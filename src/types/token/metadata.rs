use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    utils::single_flight::{with_cache, GLOBAL_CACHE},
    cache_key,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
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
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!("token_metadata", token_id);
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let token_id = token_id.to_string();
            async move {
                let controller = TokenMetadataController::new(db);
                controller.fetch_token_metadata(&token_id).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_token_metadata completed in {:?} for token_id: {}",
            elapsed, token_id
        );
        Ok(response)
    }
    
    async fn fetch_token_metadata(&self, token_id: &str) -> Result<TokenMetadataResponse> {
        let token = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, TokenMetadata>(
                "SELECT token_id, name, symbol, image_uri FROM token WHERE token_id = $1",
            )
            .bind(token_id)
            .fetch_one(&*self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(TokenMetadataResponse {
            token_metadata: token,
        })
    }
}
