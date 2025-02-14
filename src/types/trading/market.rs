use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Market {
    pub market_id: String,
    pub market_type: String,
    pub token_id: String,
    pub virtual_native: BigDecimal,
    pub virtual_token: BigDecimal,
    pub reserve_token: BigDecimal,
    pub reserve_native: BigDecimal,
    pub price: BigDecimal,
    pub latest_trade_at: i64,
    pub created_at: i64,
}

pub struct MarketController {
    pub db: Arc<PostgresDatabase>,
}

impl MarketController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MarketController { db }
    }

    pub async fn get_market_by_token(&self, token_id: &str) -> Result<Market> {
        let market = sqlx::query_as!(
            Market,
            r#"
            SELECT 
                market_id as "market_id!",
                market_type as "market_type!",
                token_id as "token_id!",
                virtual_native as "virtual_native!",
                virtual_token as "virtual_token!",
                reserve_token as "reserve_token!",
                reserve_native as "reserve_native!",
                price as "price!",
                latest_trade_at as "latest_trade_at!",
                created_at as "created_at!"
            FROM market
            WHERE token_id = $1
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(market)
    }
}
