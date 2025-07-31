use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    utils::single_flight::{with_cache, GLOBAL_CACHE},
    cache_key,
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]

pub struct PriceResponse {
    pub price: BigDecimal,
    pub token_address: String,
}

pub struct PriceController {
    pub db: Arc<PostgresDatabase>,
}

impl PriceController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PriceController { db }
    }
    pub async fn get_price(&self, token: &str) -> Result<PriceResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!("price", token);
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let token = token.to_string();
            async move {
                let controller = PriceController::new(db);
                controller.fetch_price(&token).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!("get_price completed in {:?} for token: {}", elapsed, token);
        Ok(response)
    }
    
    async fn fetch_price(&self, token: &str) -> Result<PriceResponse> {
        #[derive(FromRow)]
        struct PriceRow {
            price: BigDecimal,
        }

        let price = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, PriceRow>(
                r#"
                SELECT 
                    COALESCE(m.price, 0)::numeric as price
                FROM market m
                WHERE m.token_id = $1
                "#,
            )
            .bind(token)
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(PriceResponse {
            price: price.price,
            token_address: token.to_string(),
        })
    }
}
