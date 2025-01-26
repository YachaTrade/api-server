pub mod create_token;
pub mod order;

use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Token {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub creator: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub image_uri: String,
    pub is_listing: bool,
    pub total_supply: BigDecimal,
    pub created_at: i64,
    pub create_transaction_hash: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponse {
    pub token: Token,
}
pub struct TokenController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenController { db }
    }
    pub async fn get_token(&self, token_id: String) -> Result<Token> {
        let token = sqlx::query_as!(
            Token,
            r#"
            SELECT * FROM token WHERE LOWER(token_id) = LOWER($1)
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(token)
    }
}
