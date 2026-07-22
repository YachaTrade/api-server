use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{
                AccountInfo, BalanceInfo, MarketInfo, MarketType, QuoteInfo, TokenCreatedInfo,
                TokenInfo,
            },
            pagination::PaginationParams,
        },
        profile::CreatedTokensResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct TokenCreatedController {
    db: Arc<PostgresDatabase>,
}

impl TokenCreatedController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenCreatedController { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let cache_key = cache_key!("token_created_count", account_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = TokenCreatedController::new(db);
                controller.fetch_total_count(&account_id).await
            }
        })
        .await?;

        Ok(count)
    }

    async fn fetch_total_count(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "token_created.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT member.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                ) member
                JOIN token t ON t.token_id = member.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token created count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<CreatedTokensResponse> {
        let cache_key = cache_key!(
            "tokens_created",
            account_id,
            pagination.page,
            pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = PaginationParams {
                page: pagination.page,
                limit: pagination.limit,
                direction: pagination.direction.clone(),
            };
            async move {
                let controller = TokenCreatedController::new(db);
                controller
                    .fetch_tokens_created(&account_id, &pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<CreatedTokensResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct TokenCreatedRow {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image_uri: String,
            token_description: Option<String>,
            token_twitter: Option<String>,
            token_telegram: Option<String>,
            token_website: Option<String>,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            token_created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            market_type: MarketType,
            market_id: String,
            quote_id: String,
            token_price: BigDecimal,
            native_price: BigDecimal,
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
            balance: BigDecimal,
            balance_created_at: i64,
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
        }

        let tokens = measure_postgres!(
            "token_created.fetch_tokens_created",
            sqlx::query_as::<_, TokenCreatedRow>(
                r#"
                WITH member_ids AS (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                ),
                paged_tokens AS (
                    SELECT t.*
                    FROM token t
                    WHERE t.token_id IN (SELECT token_id FROM member_ids)
                      AND EXISTS (SELECT 1 FROM market mk WHERE mk.token_id = t.token_id)
                    ORDER BY t.created_at DESC, t.token_id ASC
                    LIMIT $2 OFFSET $3
                )
                SELECT
                    t.token_id,
                    t.name as token_name,
                    t.symbol as token_symbol,
                    t.image_uri as token_image_uri,
                    t.description as token_description,
                    t.twitter as token_twitter,
                    t.telegram as token_telegram,
                    t.website as token_website,
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                    t.created_at as token_created_at,
                    t.creator,
                    t.token_holder_count as holder_count,
                    a.nickname as creator_nickname,
                    a.bio as creator_bio,
                    a.image_uri as creator_image_uri,
                    m.market_type,
                    COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
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
                    COALESCE(b.balance, 0) as balance,
                    COALESCE(b.created_at, 0) as balance_created_at,
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
                FROM paged_tokens t
                JOIN account a ON t.creator = a.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
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
                ORDER BY t.created_at DESC, t.token_id ASC
"#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch tokens created: {}", err))?;

        let tokens: Vec<TokenCreatedInfo> = tokens
            .into_iter()
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if market_id.is_empty() && row.market_type == MarketType::Curve {
                    market_id = BONDING_CURVE.clone();
                }

                TokenCreatedInfo {
                    token_info: TokenInfo {
                        token_id: row.token_id.clone(),
                        name: row.token_name,
                        symbol: row.token_symbol,
                        image_uri: row.token_image_uri,
                        description: row.token_description,
                        is_graduated: row.is_graduated,
                        is_nsfw: row.is_nsfw,
                        twitter: row.token_twitter,
                        telegram: row.token_telegram,
                        website: row.token_website,
                        created_at: row.token_created_at,
                        creator: AccountInfo {
                            account_id: row.creator,
                            nickname: row.creator_nickname,
                            bio: row.creator_bio,
                            image_uri: row.creator_image_uri,
                        },
                        is_cto: row.is_cto,
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
                    balance_info: BalanceInfo {
                        balance: row.balance.normalized().to_plain_string(),
                        lp_balance: row.lp_balance.normalized().to_plain_string(),
                        total_balance: row.total_balance.normalized().to_plain_string(),
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
                        created_at: row.balance_created_at,
                    },
                }
            })
            .collect();

        let total_count = self.fetch_total_count(account_id).await?;

        Ok(CreatedTokensResponse {
            tokens,
            total_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const OTHER: &str = "0x000000000000000000000000000000000000Aa09";
    const CREATED_TOKEN: &str = "0x000000000000000000000000000000000000Bb01";
    const LP_TOKEN: &str = "0x000000000000000000000000000000000000Bb09";
    const POOL_ID: &str = "0x000000000000000000000000000000000000Cc09";
    const QUOTE_ID: &str = "0x4200000000000000000000000000000000000006";

    fn controller(pool: PgPool) -> TokenCreatedController {
        TokenCreatedController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn created_tokens_include_internal_lp_only_token(pool: PgPool) {
        for (account, nickname) in [(ACCOUNT, "me"), (OTHER, "other")] {
            sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,$2,'','') ON CONFLICT DO NOTHING")
                .bind(account)
                .bind(nickname)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply) VALUES ($1,'Mine','MINE','',$2,NULL,false,false,false,100,'0xh1',1000000)"#)
            .bind(CREATED_TOKEN)
            .bind(ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply) VALUES ($1,'Lp','LP','',$2,NULL,false,false,false,200,'0xh2',1000000)"#)
            .bind(LP_TOKEN)
            .bind(OTHER)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('DEX',$1,0,0,1,$2,0,0,0,0,0)"#)
            .bind(CREATED_TOKEN)
            .bind(QUOTE_ID)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO pool (pool_id,token0,token1,reserve0,reserve1,price,volume,value,latest_trade_at,created_at,block_number,tx_hash,total_supply) VALUES ($1,$2,$3,10000,20000,1,0,0,0,0,1,'0xtx',1000)"#)
            .bind(POOL_ID)
            .bind(LP_TOKEN)
            .bind("0x000000000000000000000000000000000000Bb0a")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('DEX',$1,$2,10000,20000,1,$3,0,0,0,0,0)"#)
            .bind(LP_TOKEN)
            .bind(POOL_ID)
            .bind(QUOTE_ID)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,100,0,0,0,0,0,0,0,0,0,0,0,0,0,0)"#)
            .bind(ACCOUNT)
            .bind(POOL_ID)
            .execute(&pool)
            .await
            .unwrap();

        let response = controller(pool)
            .get_tokens_created(
                ACCOUNT,
                &PaginationParams {
                    page: 1,
                    limit: 10,
                    direction: "DESC".into(),
                },
            )
            .await
            .unwrap();
        let lp = response
            .tokens
            .iter()
            .find(|token| token.token_info.token_id == LP_TOKEN)
            .expect("internal LP-only token must be included");
        assert_eq!(lp.balance_info.lp_balance, "1000");
        assert_eq!(lp.balance_info.total_balance, "1000");
        assert_eq!(response.total_count, 2);
    }
}
