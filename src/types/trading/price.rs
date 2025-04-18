use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
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
        let price = sqlx::query!(
            r#"
            SELECT 
                COALESCE(m.price, 0)::numeric as "price!"
            FROM market m
            WHERE m.token_id = $1
            "#,
            token
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(PriceResponse {
            price: price.price,
            token_address: token.to_string(),
        })
    }
}
