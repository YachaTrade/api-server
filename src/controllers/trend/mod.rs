use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo},
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
    price_24h_ago: BigDecimal,
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
            JOIN market m ON t.token_id = m.token_id
            JOIN quote_token qt ON m.quote_id = qt.quote_id
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
        if market_id.is_empty() && row.market_type == MarketType::Curve {
            market_id = BONDING_CURVE.clone();
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
            },
            market_info: MarketInfo {
                market_type: row.market_type,
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
            },
            percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const TOKEN_A: &str = "0xA000000000000000000000000000000000000A";
    const TOKEN_B: &str = "0xB000000000000000000000000000000000000B";
    const TOKEN_C: &str = "0xC000000000000000000000000000000000000C";
    const TOKEN_D: &str = "0xD000000000000000000000000000000000000D";

    fn controller(pool: PgPool) -> TrendController {
        TrendController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn rows(pool: &PgPool) -> Vec<(String, i32)> {
        sqlx::query_as("SELECT token_id, display_order FROM trend ORDER BY display_order")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_replaces_all_zero_based(pool: PgPool) {
        controller(pool.clone())
            .insert_trend_token(TrendRequest {
                token_ids: vec![TOKEN_A.into(), TOKEN_B.into(), TOKEN_C.into()],
            })
            .await
            .unwrap();

        assert_eq!(
            rows(&pool).await,
            vec![
                (TOKEN_A.into(), 0),
                (TOKEN_B.into(), 1),
                (TOKEN_C.into(), 2),
            ]
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_reorders_overlap_without_duplicate_key(pool: PgPool) {
        let controller = controller(pool.clone());
        controller
            .insert_trend_token(TrendRequest {
                token_ids: vec![TOKEN_A.into(), TOKEN_B.into(), TOKEN_C.into()],
            })
            .await
            .unwrap();
        controller
            .insert_trend_token(TrendRequest {
                token_ids: vec![TOKEN_C.into(), TOKEN_A.into(), TOKEN_D.into()],
            })
            .await
            .unwrap();

        assert_eq!(
            rows(&pool).await,
            vec![
                (TOKEN_C.into(), 0),
                (TOKEN_A.into(), 1),
                (TOKEN_D.into(), 2),
            ]
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_trend_empty_list_clears_all(pool: PgPool) {
        let controller = controller(pool.clone());
        controller
            .insert_trend_token(TrendRequest {
                token_ids: vec![TOKEN_A.into(), TOKEN_B.into()],
            })
            .await
            .unwrap();
        controller
            .insert_trend_token(TrendRequest { token_ids: vec![] })
            .await
            .unwrap();

        assert!(rows(&pool).await.is_empty());
    }
}
