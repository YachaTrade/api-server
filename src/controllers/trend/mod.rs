use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, FeeInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo},
        trend::{TrendActionResponse, TrendRequest, TrendResponse, TrendToken},
    },
    utils::{
        calculate_price_change_percent, current_unix_timestamp,
        single_flight::{GLOBAL_CACHE, with_cache},
    },
};

#[derive(Debug, sqlx::FromRow)]
struct TrendTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: String,
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
    price_24h_ago: BigDecimal,
    creator_fee_rate: Option<i16>,
    curve_protocol_fee_rate: Option<i16>,
    dex_protocol_fee_rate: Option<i16>,
}

pub struct TrendController {
    db: Arc<PostgresDatabase>,
}

impl TrendController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TrendController { db }
    }

    pub async fn get_trend_tokens(&self) -> Result<TrendResponse> {
        let cache_key = "trend_tokens:all";

        let tokens = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = TrendController::new(db);
                controller.fetch_trend_tokens().await
            }
        })
        .await?;

        Ok(TrendResponse { tokens })
    }

    /// Get trend tokens without single flight (used by TrendService which handles caching)
    pub async fn get_trend_tokens_raw(&self) -> Result<TrendResponse> {
        let tokens = self.fetch_trend_tokens().await?;
        Ok(TrendResponse { tokens })
    }

    async fn fetch_trend_tokens(&self) -> Result<Vec<TrendToken>> {
        let current_time = current_unix_timestamp();
        let time_24h_ago = current_time - 86400;

        let query = r#"
            SELECT
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_graduated,
                t.is_nsfw,
                t.is_cto,
                t.created_at,
                t.creator,
                t.token_holder_count as holder_count,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
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
                fc.creator_fee_rate,
                fc.curve_protocol_fee_rate,
                fc.dex_protocol_fee_rate,
                COALESCE(
                    (
                        SELECT ph.price
                        FROM price_history ph
                        WHERE ph.token_id = t.token_id
                        AND ph.created_at <= $1
                        ORDER BY
                            ph.created_at DESC,
                            ph.tx_index DESC,
                            ph.log_index DESC
                        LIMIT 1
                    ),
                    (
                        SELECT ph.price
                        FROM price_history ph
                        WHERE ph.token_id = t.token_id
                        ORDER BY
                            ph.created_at ASC,
                            ph.tx_index ASC,
                            ph.log_index ASC
                        LIMIT 1
                    )
                ) as price_24h_ago
            FROM trend tr
            JOIN token t ON tr.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            JOIN market m ON t.token_id = m.token_id
            JOIN quote_token qt ON m.quote_id = qt.quote_id
            LEFT JOIN fee_config fc ON t.token_id = fc.token_id
            LEFT JOIN LATERAL (
                SELECT p.price
                FROM price p
                WHERE p.quote_id = m.quote_id
                ORDER BY p.block_number DESC
                LIMIT 1
            ) lp ON true
            ORDER BY tr.display_order ASC
        "#;

        let rows = measure_postgres!(
            "trend.fetch_trend_tokens",
            sqlx::query_as::<_, TrendTokenRow>(query)
                .bind(time_24h_ago)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch trend tokens: {}", err))?;

        Ok(rows.into_iter().map(TrendToken::from).collect())
    }

    /// Single-statement CTE: pgbouncer runs in statement pooling mode, which
    /// forbids `BEGIN`/`COMMIT`. `del` and `ins` are disjoint by `token_id`
    /// (del only removes ids NOT in the new list; ins only upserts ids IN the
    /// new list), so this is safe regardless of the planner-chosen CTE
    /// execution order — deleting and re-inserting the SAME key within one
    /// statement would hit `duplicate key value violates unique constraint`.
    pub async fn insert_trend_token(&self, request: TrendRequest) -> Result<TrendActionResponse> {
        measure_postgres!(
            "trend.replace_all",
            sqlx::query(
                "WITH del AS (
                     DELETE FROM trend WHERE NOT (token_id = ANY($1::text[]))
                 ),
                 ins AS (
                     INSERT INTO trend (token_id, display_order)
                     SELECT u.token_id, (u.ord - 1)::int
                     FROM unnest($1::text[]) WITH ORDINALITY AS u(token_id, ord)
                     ON CONFLICT (token_id) DO UPDATE
                       SET display_order = EXCLUDED.display_order,
                           created_at = EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT
                 )
                 SELECT 1",
            )
            .bind(&request.token_ids)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to replace trends: {}", err))?;

        // Clear cache
        GLOBAL_CACHE
            .cache
            .invalidate(&"trend_tokens:all".to_string())
            .await;

        Ok(TrendActionResponse { success: true })
    }

    pub async fn is_admin(&self, account_id: &str) -> Result<bool> {
        let query = "SELECT COUNT(*) as count FROM admin WHERE account_id = $1";

        let result = measure_postgres!(
            "trend.is_admin",
            sqlx::query_scalar::<_, i64>(query)
                .bind(account_id)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check admin status: {}", err))?;

        Ok(result > 0)
    }
}

impl From<TrendTokenRow> for TrendToken {
    fn from(row: TrendTokenRow) -> Self {
        let mut market_id = row.market_id.clone();
        if market_id.is_empty() {
            if row.market_type == "CURVE" {
                market_id = V1_BONDING_CURVE.clone();
            } else if row.market_type == "V2_CURVE" {
                market_id = V2_BONDING_CURVE.clone();
            }
        }

        let percent = calculate_price_change_percent(
            &row.price_24h_ago.normalized().to_plain_string(),
            &row.price.normalized().to_plain_string(),
        )
        .unwrap_or(0.0);

        TrendToken {
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
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
                x_verification: None,
            },
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    "V2_CURVE" => MarketType::V2Curve,
                    "V2_DEX" => MarketType::V2Dex,
                    _ => MarketType::Curve,
                },
                market_id,
                token_id: row.token_id,
                quote_info: QuoteInfo {
                    quote_id: row.quote_id.clone(),
                    name: row.quote_name.clone(),
                    symbol: row.quote_symbol.clone(),
                    decimals: row.quote_decimals as u32,
                    image_uri: row.quote_image_uri.clone(),
                },
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
            percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    // trend.token_id has no FK to `token` — any VARCHAR(42)-fitting string is
    // valid storage-wise; checksum validation is a router-layer concern.
    const TOKEN_A: &str = "0xA000000000000000000000000000000000000A";
    const TOKEN_B: &str = "0xB000000000000000000000000000000000000B";
    const TOKEN_C: &str = "0xC000000000000000000000000000000000000C";
    const TOKEN_D: &str = "0xD000000000000000000000000000000000000D";

    fn make_controller(pool: PgPool) -> TrendController {
        TrendController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn trend_rows(pool: &PgPool) -> Vec<(String, i32)> {
        sqlx::query_as("SELECT token_id, display_order FROM trend ORDER BY display_order")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_replaces_all_zero_based(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        ctrl.insert_trend_token(TrendRequest {
            token_ids: vec![
                TOKEN_A.to_string(),
                TOKEN_B.to_string(),
                TOKEN_C.to_string(),
            ],
        })
        .await
        .unwrap();

        let rows = trend_rows(&pool).await;
        assert_eq!(
            rows,
            vec![
                (TOKEN_A.to_string(), 0),
                (TOKEN_B.to_string(), 1),
                (TOKEN_C.to_string(), 2),
            ]
        );
    }

    // Regression guard for the del/ins CTE split documented on
    // `insert_trend_token`: resubmitting an overlapping-but-reordered set of
    // token_ids must NOT hit `duplicate key value violates unique
    // constraint`. A naive DELETE-all-then-INSERT-all CTE would try to
    // delete AND (re)insert the same surviving keys (C, A) within a single
    // statement, which errors under Postgres's data-modifying-CTE rules —
    // this proves the disjoint upsert+delete implementation avoids that.
    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_reorder_with_overlap_does_not_duplicate_key(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        ctrl.insert_trend_token(TrendRequest {
            token_ids: vec![
                TOKEN_A.to_string(),
                TOKEN_B.to_string(),
                TOKEN_C.to_string(),
            ],
        })
        .await
        .unwrap();

        let result = ctrl
            .insert_trend_token(TrendRequest {
                token_ids: vec![
                    TOKEN_C.to_string(),
                    TOKEN_A.to_string(),
                    TOKEN_D.to_string(),
                ],
            })
            .await;
        assert!(
            result.is_ok(),
            "reorder with overlap must not error: {:?}",
            result.err()
        );

        let rows = trend_rows(&pool).await;
        assert_eq!(
            rows,
            vec![
                (TOKEN_C.to_string(), 0),
                (TOKEN_A.to_string(), 1),
                (TOKEN_D.to_string(), 2),
            ],
            "B dropped, C/A reordered, D added"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_empty_list_clears_all(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        ctrl.insert_trend_token(TrendRequest {
            token_ids: vec![TOKEN_A.to_string(), TOKEN_B.to_string()],
        })
        .await
        .unwrap();

        ctrl.insert_trend_token(TrendRequest { token_ids: vec![] })
            .await
            .unwrap();

        let rows = trend_rows(&pool).await;
        assert!(rows.is_empty());
    }
}
