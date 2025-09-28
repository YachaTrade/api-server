use std::{env, sync::Arc};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
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
            price: raw.price.to_plain_string(),
            total_supply: raw.total_supply.to_plain_string(),
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
        // 캐시 키 생성
        let cache_key = cache_key!("market", token_id);

        // Single Flight Pattern 적용
        let market = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_market_by_token(token_id).await
        })
        .await?;

        Ok(market)
    }

    async fn fetch_market_by_token(&self, token_id: &str) -> Result<Market> {
        let market = measure_postgres!(
            "market.fetch_market_by_token",
            sqlx::query_as::<_, MarketRow>(
                r#"
                SELECT 
                    m.market_type,
                    m.token_id,
                    m.pool_id,
                    m.price,
                    t.total_supply
                FROM market m
                JOIN token t ON m.token_id = t.token_id
                WHERE m.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch market by token: {}", err))?;

        Ok(Market::from(market))
    }
}
