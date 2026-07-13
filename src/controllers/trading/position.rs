use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{
            AccountInfo, BalanceInfo, FeeInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo,
            TokenVersion, TokenWithBalanceInfo,
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

    pub async fn get_total_count_by_token_holder(
        &self,
        token_id: &str,
        v1_owners: &[String],
    ) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_token_holder",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT h.account_id)::bigint as count
                FROM (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                    UNION
                    SELECT account_id FROM unnest($2::varchar[]) AS u(account_id)
                ) h
                JOIN account a ON a.account_id = h.account_id
                -- dead burn address is INCLUDED (mirrors the holder list); only the
                -- DividendVault is excluded so the count matches the listed rows.
                -- Empty $3 (env unset) is a no-op.
                WHERE a.account_id <> $3
                "#,
            )
            .bind(token_id)
            .bind(v1_owners)
            .bind(crate::config::V2_DIVIDEND_VAULT.as_str())
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token holder count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
        v1_lp: &[(String, bigdecimal::BigDecimal)],
    ) -> Result<TokenHolderResponse> {
        let cache_key = cache_key!("token_holders", token_id, pagination.page, pagination.limit);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_holders_by_token(token_id, pagination, v1_lp)
                .await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
        v1_lp: &[(String, bigdecimal::BigDecimal)],
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let v1_owners: Vec<String> = v1_lp.iter().map(|(o, _)| o.clone()).collect();
        let v1_amts: Vec<bigdecimal::BigDecimal> = v1_lp.iter().map(|(_, a)| a.clone()).collect();

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
                WITH v1_lp AS (
                    SELECT account_id, amt
                    FROM unnest($4::varchar[], $5::numeric[]) AS u(account_id, amt)
                ),
                holders AS (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                    UNION
                    SELECT account_id FROM v1_lp
                )
                SELECT
                    COALESCE(b.balance, 0) as balance,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    a.account_id,
                    COALESCE(ax.x_handle, a.nickname) as nickname,
                    a.bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    (
                      COALESCE(FLOOR(CASE WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                         WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                         ELSE 0 END), 0)
                      + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(FLOOR(CASE WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                            WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                            ELSE 0 END), 0)
                      + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM holders h
                JOIN account a ON a.account_id = h.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON m.token_id = $1
                LEFT JOIN balance b ON b.token_id = $1 AND b.account_id = h.account_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = h.account_id
                LEFT JOIN v1_lp v1 ON v1.account_id = h.account_id
                LEFT JOIN LATERAL (
                    SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC LIMIT 1
                ) lp ON true
                -- The dead burn address is intentionally INCLUDED (shown as a
                -- holder so burned supply surfaces). Only the singleton DividendVault
                -- is hidden (holds source tokens as dividends, not a real holder).
                -- Empty $6 (env unset) is a no-op.
                WHERE a.account_id <> $6
                ORDER BY total_balance DESC, a.account_id ASC
                OFFSET $2 LIMIT $3
                "#,
            )
            .bind(token_id)
            .bind(offset)
            .bind(pagination.limit)
            .bind(&v1_owners)
            .bind(&v1_amts)
            .bind(crate::config::V2_DIVIDEND_VAULT.as_str())
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holders: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_token_holder(token_id, &v1_owners)
                .await?
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

    pub async fn get_total_count_by_hold_token(
        &self,
        account_id: &str,
        v1_ids: &[String],
    ) -> Result<i64> {
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
                    UNION
                    SELECT token_id FROM unnest($2::varchar[]) AS u(token_id)
                ) h
                JOIN token t  ON t.token_id = h.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .bind(v1_ids)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        v1_lp: &[(String, bigdecimal::BigDecimal)],
    ) -> Result<HoldTokenResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let v1_ids: Vec<String> = v1_lp.iter().map(|(id, _)| id.clone()).collect();
        let v1_amts: Vec<bigdecimal::BigDecimal> = v1_lp.iter().map(|(_, a)| a.clone()).collect();

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
            version: TokenVersion,
            created_at: i64,
            creator: String,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            balance: BigDecimal,
            balance_created_at: i64,
            token_price: BigDecimal,
            native_price: BigDecimal,
            market_type: String,
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
            creator_fee_rate: Option<i16>,
            curve_protocol_fee_rate: Option<i16>,
            dex_protocol_fee_rate: Option<i16>,
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
        }

        let records = measure_postgres!(
            "position.get_hold_token_by_account",
            sqlx::query_as::<_, HoldTokenRow>(
                r#"
                WITH v1_lp AS (
                    SELECT token_id, amt
                    FROM unnest($4::varchar[], $5::numeric[]) AS u(token_id, amt)
                ),
                held AS (
                    SELECT token_id FROM balance WHERE account_id = $1 AND balance > 0
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM v1_lp
                ),
                hold_token_rows AS (
                    SELECT
                    t.token_id, t.name, t.symbol, t.image_uri, t.description,
                    t.twitter, t.telegram, t.website, t.is_graduated, t.is_nsfw, t.is_cto,
                    t.version, t.created_at, t.creator,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
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
                    m.volume, m.ath_price, m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name,
                    COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals,
                    COALESCE(qt.image_uri, '') as quote_image_uri,
                    t.token_holder_count as holder_count,
                    fc.creator_fee_rate, fc.curve_protocol_fee_rate, fc.dex_protocol_fee_rate,
                    (
                      COALESCE(FLOOR(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                         WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                         ELSE 0 END), 0)
                      + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(FLOOR(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                            WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                            ELSE 0 END), 0)
                      + COALESCE(v1.amt, 0)
                    ) AS total_balance
                    FROM held h
                    JOIN token t  ON t.token_id = h.token_id
                    JOIN market m ON m.token_id = t.token_id
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                    LEFT JOIN balance b ON b.token_id = t.token_id AND b.account_id = $1
                    LEFT JOIN pool ON pool.pool_id = m.pool_id
                    LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                    LEFT JOIN v1_lp v1 ON v1.token_id = t.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC LIMIT 1
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
            .bind(&v1_ids)
            .bind(&v1_amts)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token by account: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_hold_token(account_id, &v1_ids)
                .await?
        };

        let tokens = records
            .into_iter()
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if market_id.is_empty() {
                    if row.market_type == "CURVE" {
                        market_id = V1_BONDING_CURVE.clone();
                    } else if row.market_type == "V2_CURVE" {
                        market_id = V2_BONDING_CURVE.clone();
                    }
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
                        version: row.version.clone(),
                        x_verification: None,
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
                        market_type: match row.market_type.as_str() {
                            "CURVE" => MarketType::Curve,
                            "DEX" => MarketType::Dex,
                            "V2_CURVE" => MarketType::V2Curve,
                            "V2_DEX" => MarketType::V2Dex,
                            _ => MarketType::Curve,
                        },
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
                        fee_info: match row.market_type.as_str() {
                            "V2_CURVE" | "V2_DEX" => Some(FeeInfo {
                                creator_protocol_fee_rate: row.creator_fee_rate.unwrap_or(0),
                                curve_protocol_fee_rate: row.curve_protocol_fee_rate.unwrap_or(0),
                                dex_protocol_fee_rate: row.dex_protocol_fee_rate.unwrap_or(0),
                            }),
                            _ => None,
                        },
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
    use std::str::FromStr;

    // EIP-55 checksummed addresses
    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const ACCOUNT2: &str = "0x000000000000000000000000000000000000Aa02"; // no-LP holder
    const TOKEN_ID: &str = "0x000000000000000000000000000000000000Bb01";
    const TOKEN1_ID: &str = "0x000000000000000000000000000000000000Bb02"; // pool token1 (quote side)
    const TOKEN2_ID: &str = "0x000000000000000000000000000000000000bB03";
    const POOL_ID: &str = "0x000000000000000000000000000000000000Cc01";
    const QUOTE_ID: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A"; // default MON
    const QUOTE2_ID: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

    fn make_controller(pool: PgPool) -> PositionController {
        PositionController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    /// Insert minimal fixtures for a V2_DEX token with an LP position.
    /// lp_in=100, lp_out=0 → balance=100
    /// pool: reserve0=10000 (token0=TOKEN_ID), reserve1=20000, total_supply=1000
    /// expected lp_balance = 100 * 10000 / 1000 = 1000
    async fn seed_v2_dex(pool: &PgPool) {
        // account
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri) VALUES ($1, 'tester', '', '') ON CONFLICT DO NOTHING",
        )
        .bind(ACCOUNT)
        .execute(pool)
        .await
        .unwrap();

        // token (V2)
        sqlx::query(
            r#"INSERT INTO token (token_id, name, symbol, image_uri, creator, description, is_nsfw, is_graduated, is_cto, created_at, transaction_hash, total_supply, version)
               VALUES ($1, 'TestTok', 'TTK', '', $2, NULL, false, false, false, 0, '0xhash', 1000000, 'V2')
               ON CONFLICT DO NOTHING"#,
        )
        .bind(TOKEN_ID)
        .bind(ACCOUNT)
        .execute(pool)
        .await
        .unwrap();

        // swap_count (required FK)
        sqlx::query(
            "INSERT INTO swap_count (token_id, count, buy_count, sell_count) VALUES ($1, 0, 0, 0) ON CONFLICT DO NOTHING",
        )
        .bind(TOKEN_ID)
        .execute(pool)
        .await
        .unwrap();

        // pool (token0 = TOKEN_ID, reserve0 = 10000, total_supply = 1000)
        sqlx::query(
            r#"INSERT INTO pool (pool_id, token0, token1, reserve0, reserve1, price, volume, value, latest_trade_at, created_at, block_number, tx_hash, total_supply)
               VALUES ($1, $2, $3, 10000, 20000, 1, 0, 0, 0, 0, 1, '0xtx', 1000)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(POOL_ID)
        .bind(TOKEN_ID)
        .bind(TOKEN1_ID)
        .execute(pool)
        .await
        .unwrap();

        // market (V2_DEX with pool_id)
        sqlx::query(
            r#"INSERT INTO market (market_type, token_id, pool_id, reserve_token, reserve_quote, price, quote_id, latest_trade_at, created_at, volume, ath_price, ath_price_quote)
               VALUES ('V2_DEX', $1, $2, 10000, 20000, 1, $3, 0, 0, 0, 0, 0)
               ON CONFLICT (token_id) DO NOTHING"#,
        )
        .bind(TOKEN_ID)
        .bind(POOL_ID)
        .bind(QUOTE_ID)
        .execute(pool)
        .await
        .unwrap();

        // balance (account holds token)
        sqlx::query(
            "INSERT INTO balance (account_id, token_id, balance, created_at) VALUES ($1, $2, 500, 0) ON CONFLICT DO NOTHING",
        )
        .bind(ACCOUNT)
        .bind(TOKEN_ID)
        .execute(pool)
        .await
        .unwrap();

        // lp_position: lp_in=100, lp_out=0 → balance=100
        sqlx::query(
            r#"INSERT INTO lp_position
                   (account_id, pool_id, lp_in, lp_out,
                    token0_in, token0_out, token1_in, token1_out,
                    token0_in_usd, token0_out_usd, token1_in_usd, token1_out_usd,
                    created_at, updated_at,
                    epoch_start_block, epoch_start_tx_index, epoch_start_log_index)
               VALUES ($1, $2, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(ACCOUNT)
        .bind(POOL_ID)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_wallet_holding(
        pool: &PgPool,
        token_id: &str,
        quote_id: &str,
        balance: &str,
        market_price: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO token (token_id, name, symbol, image_uri, creator, description, is_nsfw, is_graduated, is_cto, created_at, transaction_hash, total_supply, version)
               VALUES ($1, 'WalletTok', 'WTK', '', $2, NULL, false, false, false, 0, $1, 1000000, 'V2')"#,
        )
        .bind(token_id)
        .bind(ACCOUNT)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO market (market_type, token_id, reserve_token, reserve_quote, price, quote_id, latest_trade_at, created_at, volume, ath_price, ath_price_quote)
               VALUES ('V2_CURVE', $1, 0, 0, $2::numeric, $3, 0, 0, 0, 0, 0)"#,
        )
        .bind(token_id)
        .bind(market_price)
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO balance (account_id, token_id, balance, created_at) VALUES ($1, $2, $3::numeric, 0)",
        )
        .bind(ACCOUNT)
        .bind(token_id)
        .bind(balance)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn lp_balance_v2_dex_computed_correctly(pool: PgPool) {
        seed_v2_dex(&pool).await;

        let ctrl = make_controller(pool);
        let pagination = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &pagination, &[])
            .await
            .unwrap();

        assert_eq!(resp.tokens.len(), 1, "expected 1 hold token");
        let token = &resp.tokens[0];

        // lp_balance = lp_pos.balance * reserve0 / pool.total_supply
        // = 100 * 10000 / 1000 = 1000
        let lp_bal = BigDecimal::from_str(&token.balance_info.lp_balance).unwrap();
        assert_eq!(
            lp_bal,
            BigDecimal::from(1000),
            "lp_balance mismatch: got {}",
            lp_bal
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn lp_balance_zero_when_no_lp_position(pool: PgPool) {
        seed_v2_dex(&pool).await;
        // Remove lp_position to simulate no LP stake
        sqlx::query("DELETE FROM lp_position WHERE account_id = $1")
            .bind(ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();

        let ctrl = make_controller(pool);
        let pagination = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &pagination, &[])
            .await
            .unwrap();

        assert_eq!(resp.tokens[0].balance_info.lp_balance, "0");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn lp_balance_zero_when_total_supply_zero(pool: PgPool) {
        seed_v2_dex(&pool).await;
        // Set pool total_supply to 0
        sqlx::query("UPDATE pool SET total_supply = 0 WHERE pool_id = $1")
            .bind(POOL_ID)
            .execute(&pool)
            .await
            .unwrap();

        let ctrl = make_controller(pool);
        let pagination = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &pagination, &[])
            .await
            .unwrap();

        assert_eq!(resp.tokens[0].balance_info.lp_balance, "0");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_computes_total_balance(pool: PgPool) {
        seed_v2_dex(&pool).await; // ACCOUNT: balance 500, lp_balance 1000 -> total 1500
        let ctrl = make_controller(pool);
        let p = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &p, &[])
            .await
            .unwrap();
        let tok = &resp.tokens[0];
        assert_eq!(tok.balance_info.balance, "500");
        assert_eq!(tok.balance_info.lp_balance, "1000");
        assert_eq!(tok.balance_info.total_balance, "1500");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_sorts_by_usd_value_not_raw_balance(pool: PgPool) {
        seed_v2_dex(&pool).await; // 1,500 tokens * $2 = $3,000
        sqlx::query("UPDATE market SET price = 2 WHERE token_id = $1")
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        seed_wallet_holding(&pool, TOKEN2_ID, QUOTE_ID, "2000", "0.5").await; // $1,000
        sqlx::query("INSERT INTO price (quote_id, block_number, price) VALUES ($1, 999, 1)")
            .bind(QUOTE_ID)
            .execute(&pool)
            .await
            .unwrap();

        let resp = make_controller(pool)
            .get_hold_token_by_account(
                ACCOUNT,
                &PaginationParams {
                    page: 1,
                    limit: 10,
                    direction: "DESC".to_string(),
                },
                &[],
            )
            .await
            .unwrap();

        assert_eq!(resp.tokens[0].token_info.token_id, TOKEN_ID);
        assert_eq!(resp.tokens[1].token_info.token_id, TOKEN2_ID);
        assert_eq!(resp.tokens[0].balance_info.total_balance, "1500");
        assert_eq!(resp.tokens[1].balance_info.total_balance, "2000");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_usd_value_ties_sort_by_token_id(pool: PgPool) {
        seed_v2_dex(&pool).await; // 1,500 tokens * $2 = $3,000
        sqlx::query("UPDATE market SET price = 2 WHERE token_id = $1")
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        seed_wallet_holding(&pool, TOKEN2_ID, QUOTE_ID, "6000", "0.5").await; // $3,000
        sqlx::query("INSERT INTO price (quote_id, block_number, price) VALUES ($1, 999, 1)")
            .bind(QUOTE_ID)
            .execute(&pool)
            .await
            .unwrap();

        let resp = make_controller(pool)
            .get_hold_token_by_account(
                ACCOUNT,
                &PaginationParams {
                    page: 1,
                    limit: 10,
                    direction: "DESC".to_string(),
                },
                &[],
            )
            .await
            .unwrap();

        assert_eq!(resp.tokens[0].token_info.token_id, TOKEN_ID);
        assert_eq!(resp.tokens[1].token_info.token_id, TOKEN2_ID);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_missing_quote_price_has_zero_usd_value(pool: PgPool) {
        seed_v2_dex(&pool).await;
        sqlx::query("INSERT INTO price (quote_id, block_number, price) VALUES ($1, 999, 1)")
            .bind(QUOTE_ID)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO quote_token (quote_id, name, symbol, decimals, pyth_feed_id, image_uri)
               VALUES ($1, 'USD Coin', 'USDC', 6, 'test-feed', '')"#,
        )
        .bind(QUOTE2_ID)
        .execute(&pool)
        .await
        .unwrap();
        seed_wallet_holding(&pool, TOKEN2_ID, QUOTE2_ID, "999999999", "999").await;

        let resp = make_controller(pool)
            .get_hold_token_by_account(
                ACCOUNT,
                &PaginationParams {
                    page: 1,
                    limit: 10,
                    direction: "DESC".to_string(),
                },
                &[],
            )
            .await
            .unwrap();

        assert_eq!(resp.tokens[0].token_info.token_id, TOKEN_ID);
        assert_eq!(resp.tokens[1].token_info.token_id, TOKEN2_ID);
        assert_eq!(resp.tokens[1].balance_info.token_price, "0");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_includes_lp_only_v2(pool: PgPool) {
        seed_v2_dex(&pool).await;
        sqlx::query("DELETE FROM balance WHERE account_id=$1 AND token_id=$2")
            .bind(ACCOUNT)
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &p, &[])
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1, "LP-only token must be present");
        assert_eq!(resp.tokens[0].balance_info.balance, "0");
        assert_eq!(resp.tokens[0].balance_info.lp_balance, "1000");
        assert_eq!(resp.tokens[0].balance_info.total_balance, "1000");
        assert_eq!(resp.total_count, 1);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_includes_lp_only_v1_injected(pool: PgPool) {
        seed_v2_dex(&pool).await;
        sqlx::query("UPDATE token SET version='V1' WHERE token_id=$1")
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM lp_position WHERE account_id=$1")
            .bind(ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM balance WHERE account_id=$1 AND token_id=$2")
            .bind(ACCOUNT)
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let v1 = vec![(TOKEN_ID.to_string(), bigdecimal::BigDecimal::from(42))];
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &p, &v1)
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].balance_info.lp_balance, "42");
        assert_eq!(resp.tokens[0].balance_info.total_balance, "42");
    }

    // ── Holder V2 lp_balance tests (get_holders_by_token) ────────────────────
    //
    // seed_v2_dex already inserts ACCOUNT with an lp_position.
    // These tests add ACCOUNT2 (no LP) and call get_holders_by_token(TOKEN_ID).

    /// Seed ACCOUNT2: balance only, no lp_position.
    async fn seed_account2_balance(pool: &PgPool) {
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri) VALUES ($1, 'holder2', '', '') ON CONFLICT DO NOTHING",
        )
        .bind(ACCOUNT2)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO balance (account_id, token_id, balance, created_at) VALUES ($1, $2, 200, 0) ON CONFLICT DO NOTHING",
        )
        .bind(ACCOUNT2)
        .bind(TOKEN_ID)
        .execute(pool)
        .await
        .unwrap();
    }

    /// A holder with an lp_position for a V2_DEX pool gets
    /// lp_balance = lp_pos.balance * reserve / total_supply = 100 * 10000 / 1000 = 1000.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_lp_balance_v2_dex_computed_correctly(pool: PgPool) {
        seed_v2_dex(&pool).await;
        seed_account2_balance(&pool).await;

        let ctrl = make_controller(pool);
        // Unique limit (11) to avoid sharing a GLOBAL_CACHE key with other holder tests.
        let pagination = PaginationParams {
            page: 1,
            limit: 11,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_holders_by_token(TOKEN_ID, &pagination, &[])
            .await
            .unwrap();

        assert_eq!(resp.holders.len(), 2, "expected 2 holders");

        // ACCOUNT has the larger balance (500) so it appears first after ORDER BY balance DESC.
        let holder_with_lp = resp
            .holders
            .iter()
            .find(|h| h.account_info.account_id == ACCOUNT)
            .expect("ACCOUNT not found in holders");

        // lp_balance = 100 * 10000 / 1000 = 1000
        let lp_bal = BigDecimal::from_str(&holder_with_lp.balance_info.lp_balance).unwrap();
        assert_eq!(
            lp_bal,
            BigDecimal::from(1000),
            "lp_balance mismatch for ACCOUNT: got {}",
            lp_bal
        );
    }

    /// A holder with no lp_position for the V2_DEX pool shows lp_balance = "0".
    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_lp_balance_zero_when_no_lp_position(pool: PgPool) {
        seed_v2_dex(&pool).await;
        seed_account2_balance(&pool).await;

        let ctrl = make_controller(pool);
        // Unique limit (12) to avoid sharing a GLOBAL_CACHE key with other holder tests.
        let pagination = PaginationParams {
            page: 1,
            limit: 12,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_holders_by_token(TOKEN_ID, &pagination, &[])
            .await
            .unwrap();

        let holder_no_lp = resp
            .holders
            .iter()
            .find(|h| h.account_info.account_id == ACCOUNT2)
            .expect("ACCOUNT2 not found in holders");

        assert_eq!(
            holder_no_lp.balance_info.lp_balance, "0",
            "expected lp_balance='0' for holder with no LP, got {}",
            holder_no_lp.balance_info.lp_balance
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_includes_lp_only_owner_v2(pool: PgPool) {
        seed_v2_dex(&pool).await; // ACCOUNT: balance 500 + lp 1000 -> total 1500
        let acct3 = "0x000000000000000000000000000000000000Aa03";
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'h3','','') ON CONFLICT DO NOTHING").bind(acct3).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,50,0,0,0,0,0,0,0,0,0,0,0,0,0,0) ON CONFLICT DO NOTHING"#).bind(acct3).bind(POOL_ID).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        // Use limit=50 to get a unique cache key, avoiding cross-test cache pollution
        let p = PaginationParams {
            page: 1,
            limit: 50,
            direction: "DESC".to_string(),
        };
        let resp = ctrl.get_holders_by_token(TOKEN_ID, &p, &[]).await.unwrap();
        let acct3_row = resp
            .holders
            .iter()
            .find(|h| h.account_info.account_id == acct3)
            .expect("lp-only holder present");
        assert_eq!(acct3_row.balance_info.balance, "0");
        assert_eq!(acct3_row.balance_info.lp_balance, "500"); // 50*10000/1000
        assert_eq!(acct3_row.balance_info.total_balance, "500");
        assert_eq!(resp.total_count, 2);
        assert_eq!(resp.holders[0].account_info.account_id, ACCOUNT); // total_balance DESC (1500 first)
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_includes_lp_only_owner_v1_injected(pool: PgPool) {
        seed_v2_dex(&pool).await;
        sqlx::query("UPDATE token SET version='V1' WHERE token_id=$1")
            .bind(TOKEN_ID)
            .execute(&pool)
            .await
            .unwrap();
        let acct4 = "0x000000000000000000000000000000000000Aa04";
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'h4','','') ON CONFLICT DO NOTHING").bind(acct4).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        // Use limit=100 to get a unique cache key, avoiding cross-test cache pollution
        let p = PaginationParams {
            page: 1,
            limit: 100,
            direction: "DESC".to_string(),
        };
        let v1 = vec![(acct4.to_string(), bigdecimal::BigDecimal::from(777))];
        let resp = ctrl.get_holders_by_token(TOKEN_ID, &p, &v1).await.unwrap();
        let row = resp
            .holders
            .iter()
            .find(|h| h.account_info.account_id == acct4)
            .expect("v1 lp-only holder present");
        assert_eq!(row.balance_info.lp_balance, "777");
        assert_eq!(row.balance_info.total_balance, "777");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_v2_lp_balance_is_floored(pool: PgPool) {
        seed_v2_dex(&pool).await;
        // 100 * 10001 / 1000 = 1000.1 → floor → 1000 (Uniswap V2 integer division)
        sqlx::query("UPDATE pool SET reserve0 = 10001 WHERE pool_id = $1")
            .bind(POOL_ID)
            .execute(&pool)
            .await
            .unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl
            .get_hold_token_by_account(ACCOUNT, &p, &[])
            .await
            .unwrap();
        assert_eq!(resp.tokens[0].balance_info.lp_balance, "1000"); // floored, NOT "1000.1"
        assert!(
            !resp.tokens[0].balance_info.lp_balance.contains('.'),
            "lp_balance must be integer"
        );
    }

    /// The burn address (0x…dEaD) is INCLUDED in the holder list and count so that
    /// burned supply surfaces as a holder. (Only the DividendVault is hidden.)
    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_includes_burn_address(pool: PgPool) {
        seed_v2_dex(&pool).await; // ACCOUNT is a holder (balance + lp)
        let dead = "0x000000000000000000000000000000000000dEaD";
        sqlx::query(
            "INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'burn','','') ON CONFLICT DO NOTHING",
        )
        .bind(dead)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO balance (account_id, token_id, balance, created_at) VALUES ($1, $2, 999, 0) ON CONFLICT DO NOTHING",
        )
        .bind(dead)
        .bind(TOKEN_ID)
        .execute(&pool)
        .await
        .unwrap();

        let ctrl = make_controller(pool);
        let p = PaginationParams {
            page: 1,
            limit: 13,
            direction: "DESC".to_string(),
        };
        let resp = ctrl.get_holders_by_token(TOKEN_ID, &p, &[]).await.unwrap();

        assert!(
            resp.holders
                .iter()
                .any(|h| h.account_info.account_id == dead),
            "burn address must appear in holder list"
        );
        // ACCOUNT + burn address
        assert_eq!(resp.total_count, 2, "total_count must include burn address");
    }
}
