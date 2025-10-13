use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{MarketInfo, MarketType},
        trading::market::MarketResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(sqlx::FromRow)]
struct MarketRow {
    market_type: String,
    token_id: String,
    market_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    total_supply: BigDecimal,
}

pub struct MarketController {
    pub db: Arc<PostgresDatabase>,
}

impl MarketController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MarketController { db }
    }

    pub async fn get_market_by_token(&self, token_id: &str) -> Result<MarketResponse> {
        let cache_key = cache_key!("market", token_id);

        let market = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_market_by_token(token_id).await
        })
        .await?;

        Ok(market)
    }

    async fn fetch_market_by_token(&self, token_id: &str) -> Result<MarketResponse> {
        let row = measure_postgres!(
            "market.fetch_market_by_token",
            sqlx::query_as::<_, MarketRow>(
                r#"
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                )
                SELECT
                    m.market_type,
                    m.token_id,
                    COALESCE(m.pool_id, '') as market_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    m.price,
                    t.total_supply
                FROM market m
                JOIN token t ON m.token_id = t.token_id
                CROSS JOIN latest_price lp
                WHERE m.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch market by token: {}", err))?;

        let mut market_id = row.market_id;
        if row.market_type == "CURVE" && market_id.is_empty() {
            market_id = BONDING_CURVE.clone();
        }

        Ok(MarketResponse {
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    _ => MarketType::Curve,
                },
                token_id: row.token_id,
                market_id,
                token_price: row.token_price.to_plain_string(),
                native_price: row.native_price.to_plain_string(),
                price: row.price.to_plain_string(),
                total_supply: row.total_supply.to_plain_string(),
            },
        })
    }
}
