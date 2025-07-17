use std::{env, sync::Arc, time::{Duration, Instant}};

use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MarketRaw {
    pub market_type: String,
    pub token_id: String,
    pub pool_id: Option<String>,
    pub price: BigDecimal,
    pub latest_trade_at: i64,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Market {
    pub market_type: String,
    pub token_id: String,
    pub market_id: Option<String>,
    pub price: BigDecimal,
    pub latest_trade_at: i64,
    pub created_at: i64,
}

impl From<MarketRaw> for Market {
    fn from(raw: MarketRaw) -> Self {
        let mut market = Market {
            market_type: raw.market_type,
            token_id: raw.token_id,
            market_id: raw.pool_id,
            price: raw.price,
            latest_trade_at: raw.latest_trade_at,
            created_at: raw.created_at,
        };

        if market.market_type == "CURVE" {
            market.market_id = env::var("BONDING_CURVE").ok();
        }

        market
    }
}
pub struct MarketController {
    pub db: Arc<PostgresDatabase>,
}

impl MarketController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MarketController { db }
    }

    pub async fn get_market_by_token(&self, token_id: &str) -> Result<Market> {
        let start_time = Instant::now();
        let market = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as!(
                MarketRaw,
                r#"
                SELECT 
                    market_type,
                    token_id,
                    pool_id,
                    price,
                    latest_trade_at,
                    created_at
                FROM market
                WHERE token_id = $1
                "#,
                token_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        let elapsed = start_time.elapsed();
        info!("get_market_by_token completed in {:?} for token_id: {}", elapsed, token_id);
        Ok(Market::from(market))
    }
}
