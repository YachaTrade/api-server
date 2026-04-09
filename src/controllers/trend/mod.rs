use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    config::V1_BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, MarketInfo, MarketType, TokenInfo},
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
    version: String,
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: String,
    market_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    price_usd: BigDecimal,
    total_supply: BigDecimal,
    reserve_native: BigDecimal,
    reserve_token: BigDecimal,
    volume: BigDecimal,
    ath_price: BigDecimal,
    ath_price_native: BigDecimal,
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
            WITH latest_price AS (
                SELECT price
                FROM price
                ORDER BY created_at DESC
                LIMIT 1
            )
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
                        t.version,
                t.created_at,
                t.creator,
                t.token_holder_count as holder_count,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                m.market_type,
                COALESCE(m.pool_id, '') as market_id,
                (m.price * COALESCE(lp.price, 0)) as token_price,
                COALESCE(lp.price, 0) as native_price,
                m.price,
                (m.price * COALESCE(lp.price, 0)) as price_usd,
                t.total_supply,
                COALESCE(m.reserve_native, 0) as reserve_native,
                COALESCE(m.reserve_token, 0) as reserve_token,
                m.volume,
                m.ath_price,
                m.ath_price_native,
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
            CROSS JOIN latest_price lp
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
        // Start transaction
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|err| anyhow!("Failed to start transaction: {}", err))?;

        // Delete all existing trends
        measure_postgres!(
            "trend.delete_all",
            sqlx::query("DELETE FROM trend").execute(&mut *tx)
        )
        .map_err(|err| anyhow!("Failed to delete trends: {}", err))?;

        // Insert new trends with order if not empty
        if !request.token_ids.is_empty() {
            let placeholders: Vec<String> = (0..request.token_ids.len())
                .map(|i| format!("(${}, ${})", i * 2 + 1, i * 2 + 2))
                .collect();

            let query = format!(
                "INSERT INTO trend (token_id, display_order) VALUES {}",
                placeholders.join(", ")
            );

            let mut query_builder = sqlx::query(&query);
            for (index, token_id) in request.token_ids.iter().enumerate() {
                query_builder = query_builder.bind(token_id).bind(index as i32);
            }

            measure_postgres!("trend.insert_all", query_builder.execute(&mut *tx))
                .map_err(|err| anyhow!("Failed to insert trends: {}", err))?;
        }

        // Commit transaction
        tx.commit()
            .await
            .map_err(|err| anyhow!("Failed to commit transaction: {}", err))?;

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
        if row.market_type == "CURVE" && market_id.is_empty() {
            market_id = V1_BONDING_CURVE.clone();
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
                    version: row.version.clone(),
                hackathon_info: None,
            },
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    _ => MarketType::Curve,
                },
                market_id,
                token_id: row.token_id,
                token_price: row.token_price.normalized().to_plain_string(),
                native_price: row.native_price.normalized().to_plain_string(),
                price: row.price.normalized().to_plain_string(),
                price_usd: row.price_usd.normalized().to_plain_string(),
                price_native: row.price.normalized().to_plain_string(),
                total_supply: row.total_supply.normalized().to_plain_string(),
                reserve_native: row.reserve_native.normalized().to_plain_string(),
                reserve_token: row.reserve_token.normalized().to_plain_string(),
                volume: row.volume.normalized().to_plain_string(),
                ath_price: row.ath_price.normalized().to_plain_string(),
                ath_price_usd: row.ath_price.normalized().to_plain_string(),
                ath_price_native: row.ath_price_native.normalized().to_plain_string(),
                holder_count: row.holder_count,
            },
            percent,
        }
    }
}
