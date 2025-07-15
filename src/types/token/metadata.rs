use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMetadata {
    #[serde(rename = "token_address")]
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMetadataResponse {
    pub token_metadata: TokenMetadata,
}

pub struct TokenMetadataController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenMetadataController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenMetadataController { db }
    }
    pub async fn get_token_metadata(&self, token_id: &str) -> Result<TokenMetadataResponse> {
        let token = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as!(
                TokenMetadata,
                "SELECT token_id, name, symbol, image_uri FROM token WHERE token_id = $1",
                token_id
            )
            .fetch_one(&*self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;
        Ok(TokenMetadataResponse {
            token_metadata: token,
        })
    }
}
