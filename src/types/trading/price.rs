use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

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
        let price = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query!(
                r#"
                SELECT 
                    COALESCE(m.price, 0)::numeric as "price!"
                FROM market m
                WHERE m.token_id = $1
                "#,
                token
            )
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        let elapsed = start_time.elapsed();
        info!("get_price completed in {:?} for token: {}", elapsed, token);
        Ok(PriceResponse {
            price: price.price,
            token_address: token.to_string(),
        })
    }
}
