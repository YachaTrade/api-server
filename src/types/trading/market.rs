use std::{
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
use utoipa::ToSchema;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MarketRow {
    pub market_type: String,
    pub token_id: String,
    pub pool_id: Option<String>,
    pub price: BigDecimal,
    pub total_supply: BigDecimal,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Market {
    pub market_type: String,
    pub token_id: String,
    pub market_id: Option<String>,
    pub price: String,
    pub total_supply: String,
}

impl From<MarketRow> for Market {
    fn from(raw: MarketRow) -> Self {
        let mut market = Market {
            market_type: raw.market_type,
            token_id: raw.token_id,
            market_id: raw.pool_id,
            price: raw.price.to_string(),
            total_supply: raw.total_supply.to_string(),
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

        // 캐시 키 생성
        let cache_key = cache_key!("market", token_id);

        // Single Flight Pattern 적용
        let market = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_market_by_token(token_id).await
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_market_by_token completed in {:?} for token_id: {}",
            elapsed, token_id
        );
        Ok(market)
    }

    async fn fetch_market_by_token(&self, token_id: &str) -> Result<Market> {
        let market = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, MarketRow>(
                r#"
                SELECT 
                    market_type,
                    token_id,
                    pool_id,
                    price,
                    t.total_supply
                FROM market
                JOIN token t ON market.token_id = t.token_id
                WHERE token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(Market::from(market))
    }
}
