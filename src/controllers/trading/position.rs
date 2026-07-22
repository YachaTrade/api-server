use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{
            AccountInfo, BalanceInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo,
            TokenWithBalanceInfo,
        },
        pagination::PaginationParams,
    },
    types::{
        profile::HoldTokenResponse,
        trading::position::{TokenHolder, TokenHolderResponse},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }

    pub async fn get_total_count_by_token_holder(&self, token_id: &str) -> Result<i64> {
        let cache_key = cache_key!("token_holder_count", token_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_token_holder_count(token_id).await
        })
        .await?;

        Ok(count)
    }

    async fn fetch_token_holder_count(&self, token_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "position.fetch_token_holder_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT h.account_id)::bigint as count
                FROM (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                ) h
                JOIN account a ON a.account_id = h.account_id
                "#,
            )
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holder count: {}", err))?;

        Ok(count.map(|c| c.count).unwrap_or(0))
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let cache_key = cache_key!("token_holders", token_id, pagination.page, pagination.limit);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_holders_by_token(token_id, pagination).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct TokenHolderRow {
            balance: BigDecimal,
            token_price: BigDecimal,
            native_price: BigDecimal,
            balance_created_at: i64,
            account_id: String,
            nickname: String,
            bio: String,
            image_uri: String,
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
        }

        let records = measure_postgres!(
            "position.fetch_holders_by_token",
            sqlx::query_as::<_, TokenHolderRow>(
                r#"
                WITH holders AS (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                )
                SELECT
                    COALESCE(b.balance, 0) as balance,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    a.account_id,
                    a.nickname as nickname,
                    a.bio,
                    a.image_uri as image_uri,
                    COALESCE(FLOOR(CASE
                        WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                        WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                        ELSE 0
                    END), 0) AS lp_balance,
                    COALESCE(b.balance, 0) + COALESCE(FLOOR(CASE
                        WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                        WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                        ELSE 0
                    END), 0) AS total_balance
                FROM holders h
                JOIN account a ON h.account_id = a.account_id
                JOIN market m ON m.token_id = $1
                LEFT JOIN balance b ON b.token_id = $1 AND b.account_id = h.account_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos
                    ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = h.account_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY total_balance DESC, a.account_id ASC
                OFFSET $2 LIMIT $3
                "#,
            )
            .bind(token_id)
            .bind(offset)
            .bind(pagination.limit)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holders: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_token_holder(token_id).await?
        };

        let holders = records
            .into_iter()
            .map(|row| TokenHolder {
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.nickname,
                    bio: row.bio,
                    image_uri: row.image_uri,
                },
                balance_info: BalanceInfo {
                    balance: row.balance.normalized().to_plain_string(),
                    lp_balance: row.lp_balance.normalized().to_plain_string(),
                    total_balance: row.total_balance.normalized().to_plain_string(),
                    token_price: row.token_price.normalized().to_plain_string(),
                    native_price: row.native_price.normalized().to_plain_string(),
                    quote_price: row.native_price.normalized().to_plain_string(),
                    created_at: row.balance_created_at,
                },
            })
            .collect();

        Ok(TokenHolderResponse {
            holders,
            total_count,
        })
    }

    pub async fn get_total_count_by_hold_token(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_hold_token",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT h.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM balance WHERE account_id = $1 AND balance > 0
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                ) h
                JOIN token t ON t.token_id = h.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct HoldTokenRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            description: Option<String>,
            twitter: Option<String>,
            telegram: Option<String>,
            website: Option<String>,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            created_at: i64,
            creator: String,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            balance: BigDecimal,
            balance_created_at: i64,
            token_price: BigDecimal,
            native_price: BigDecimal,
            market_type: MarketType,
            market_id: String,
            quote_id: String,
            price: BigDecimal,
            price_usd: BigDecimal,
            total_supply: BigDecimal,
            reserve_quote: BigDecimal,
            reserve_token: BigDecimal,
            volume: BigDecimal,
            ath_price: BigDecimal,
            ath_price_quote: BigDecimal,
            quote_name: String,
            quote_symbol: String,
            quote_decimals: i32,
            quote_image_uri: String,
            holder_count: i64,
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
        }

        let records = measure_postgres!(
            "position.get_hold_token_by_account",
            sqlx::query_as::<_, HoldTokenRow>(
                r#"
                WITH held AS (
                    SELECT token_id FROM balance WHERE account_id = $1 AND balance > 0
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                ),
                hold_token_rows AS (
                SELECT
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    t.description,
                    t.twitter,
                    t.telegram,
                    t.website,
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                    t.created_at,
                    t.creator,
                    a.nickname as creator_nickname,
                    a.bio as creator_bio,
                    a.image_uri as creator_image_uri,
                    COALESCE(b.balance, 0) as balance,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    m.market_type,
                    COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                    m.price,
                    (m.price * COALESCE(lp.price, 0)) as price_usd,
                    t.total_supply,
                    COALESCE(m.reserve_quote, 0) as reserve_quote,
                    COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume,
                    m.ath_price,
                    m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name,
                    COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals,
                    COALESCE(qt.image_uri, '') as quote_image_uri,
                    t.token_holder_count as holder_count,
                    COALESCE(FLOOR(CASE
                        WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                        WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                        ELSE 0
                    END), 0) AS lp_balance,
                    COALESCE(b.balance, 0) + COALESCE(FLOOR(CASE
                        WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                        WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                        ELSE 0
                    END), 0) AS total_balance
                FROM held h
                JOIN token t ON t.token_id = h.token_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN balance b ON b.token_id = t.token_id AND b.account_id = $1
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos
                    ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                )
                SELECT *
                FROM hold_token_rows
                ORDER BY (total_balance * price_usd) DESC, token_id ASC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token by account: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_hold_token(account_id).await?
        };

        let tokens = records
            .into_iter()
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if market_id.is_empty() && row.market_type == MarketType::Curve {
                    market_id = BONDING_CURVE.clone();
                }

                TokenWithBalanceInfo {
                    token_info: TokenInfo {
                        token_id: row.token_id.clone(),
                        name: row.name,
                        symbol: row.symbol,
                        image_uri: row.image_uri,
                        description: row.description,
                        is_graduated: row.is_graduated,
                        is_nsfw: row.is_nsfw,
                        twitter: row.twitter,
                        telegram: row.telegram,
                        website: row.website,
                        created_at: row.created_at,
                        creator: AccountInfo {
                            account_id: row.creator,
                            nickname: row.creator_nickname,
                            bio: row.creator_bio,
                            image_uri: row.creator_image_uri,
                        },
                        is_cto: row.is_cto,
                    },
                    balance_info: BalanceInfo {
                        balance: row.balance.normalized().to_plain_string(),
                        lp_balance: row.lp_balance.normalized().to_plain_string(),
                        total_balance: row.total_balance.normalized().to_plain_string(),
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
                        created_at: row.balance_created_at,
                    },
                    market_info: MarketInfo {
                        market_type: row.market_type,
                        token_id: row.token_id,
                        quote_info: QuoteInfo {
                            quote_id: row.quote_id.clone(),
                            name: row.quote_name.clone(),
                            symbol: row.quote_symbol.clone(),
                            decimals: row.quote_decimals as u32,
                            image_uri: row.quote_image_uri.clone(),
                        },
                        market_id,
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
                        price: row.price.normalized().to_plain_string(),
                        price_usd: row.price_usd.normalized().to_plain_string(),
                        price_native: row.price.normalized().to_plain_string(),
                        price_quote: row.price.normalized().to_plain_string(),
                        total_supply: row.total_supply.normalized().to_plain_string(),
                        reserve_native: row.reserve_quote.normalized().to_plain_string(),
                        reserve_quote: row.reserve_quote.normalized().to_plain_string(),
                        reserve_token: row.reserve_token.normalized().to_plain_string(),
                        volume: row.volume.normalized().to_plain_string(),
                        ath_price: row.ath_price.normalized().to_plain_string(),
                        ath_price_usd: row.ath_price.normalized().to_plain_string(),
                        ath_price_native: row.ath_price_quote.normalized().to_plain_string(),
                        ath_price_quote: row.ath_price_quote.normalized().to_plain_string(),
                        holder_count: row.holder_count,
                    },
                }
            })
            .collect();

        Ok(HoldTokenResponse {
            tokens,
            total_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa31";
    const TOKEN: &str = "0x000000000000000000000000000000000000Bb31";
    const OTHER_TOKEN: &str = "0x000000000000000000000000000000000000Bb32";
    const POOL: &str = "0x000000000000000000000000000000000000Cc31";
    const QUOTE: &str = "0x4200000000000000000000000000000000000006";

    fn controller(pool: PgPool) -> PositionController {
        PositionController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn internal_lp_only_position_is_a_holding_and_holder(pool: PgPool) {
        sqlx::query(
            "INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'lp','','')",
        )
        .bind(ACCOUNT)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply) VALUES ($1,'Lp','LP','',$2,NULL,false,false,false,1,'0xlp',1000000)"#)
            .bind(TOKEN)
            .bind(ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO pool (pool_id,token0,token1,reserve0,reserve1,price,volume,value,latest_trade_at,created_at,block_number,tx_hash,total_supply) VALUES ($1,$2,$3,10000,20000,1,0,0,0,0,1,'0xpool',1000)"#)
            .bind(POOL)
            .bind(TOKEN)
            .bind(OTHER_TOKEN)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('DEX',$1,$2,10000,20000,1,$3,0,0,0,0,0)"#)
            .bind(TOKEN)
            .bind(POOL)
            .bind(QUOTE)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,100,0,0,0,0,0,0,0,0,0,0,0,0,0,0)"#)
            .bind(ACCOUNT)
            .bind(POOL)
            .execute(&pool)
            .await
            .unwrap();

        let controller = controller(pool);
        let pagination = PaginationParams {
            page: 1,
            limit: 17,
            direction: "DESC".into(),
        };
        let holdings = controller
            .get_hold_token_by_account(ACCOUNT, &pagination)
            .await
            .unwrap();
        assert_eq!(holdings.tokens.len(), 1);
        assert_eq!(holdings.tokens[0].balance_info.balance, "0");
        assert_eq!(holdings.tokens[0].balance_info.lp_balance, "1000");
        assert_eq!(holdings.tokens[0].balance_info.total_balance, "1000");

        let holders = controller
            .get_holders_by_token(TOKEN, &pagination)
            .await
            .unwrap();
        assert_eq!(holders.holders.len(), 1);
        assert_eq!(holders.holders[0].account_info.account_id, ACCOUNT);
        assert_eq!(holders.holders[0].balance_info.lp_balance, "1000");
        assert_eq!(holders.total_count, 1);
    }
}
