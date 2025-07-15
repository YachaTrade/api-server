use std::sync::Arc;
use std::time::Duration;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::TokenInfo, pagination::PaginationParams},
};
use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedResponse {
    pub tokens: Vec<TokenCreated>,
    pub total_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenCreated {
    pub token: TokenInfo,
    pub is_listing: bool,
    pub created_at: i64,
    pub market_cap: BigDecimal,     //market cap -> price * total_supply
    pub total_supply: BigDecimal,   // token table total_supply
    pub price: String,              // market table price
    pub current_amount: BigDecimal, //position table current_token_amount
    pub description: Option<String>,
}

pub struct TokenCreatedController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenCreatedController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenCreatedController { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let count = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query!(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM token t
                WHERE t.creator = $1
                "#,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;
        
        let count = count.count.unwrap_or(0);

        Ok(count)
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenCreatedResponse> {
        // Query tokens created by the account with their market and position information
        let offset = (pagination.page - 1) * pagination.limit;
        let tokens = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query!(
                r#"
                WITH created_tokens AS (
                    SELECT 
                        t.token_id,
                        t.symbol,
                        t.image_uri,
                        t.name,
                        t.total_supply,
                        t.description,
                        t.created_at,
                        t.creator,
                        t.is_listing,
                        COALESCE(m.price, 0) as price,
                        COALESCE(b.balance, 0) as current_amount,
                        COALESCE(m.price * b.balance, 0) as current_value
                    FROM token t
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                    WHERE t.creator = $1
                )
                SELECT 
                    token_id,
                    symbol,
                    image_uri,
                    name,
                    is_listing as "is_listing!",
                    created_at,
                    COALESCE(price::TEXT, '0') as "price!",
                    total_supply as "total_supply!",
                    COALESCE(price * total_supply, 0) as "market_cap!",
                    COALESCE(current_amount, 0) as "current_amount!",
                    description as "description?: String"
                FROM created_tokens
                ORDER BY current_value DESC
                LIMIT $2
                OFFSET $3
                "#,
                account_id,
                pagination.limit as i64,
                offset
            )
            .fetch_all(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        // Convert query results to TokenCreated structs
        let tokens: Vec<TokenCreated> = tokens
            .into_iter()
            .map(|row| TokenCreated {
                token: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                is_listing: row.is_listing,
                created_at: row.created_at,
                market_cap: row.market_cap,
                total_supply: row.total_supply,
                price: row.price,
                current_amount: row.current_amount,
                description: row.description,
            })
            .collect();

        let total_count = self.get_total_count(account_id).await?;

        Ok(TokenCreatedResponse {
            tokens,
            total_count,
        })
    }
}
