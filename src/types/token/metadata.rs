use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;
use tracing::info;
use utoipa::ToSchema;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct TokenMetadata {
    #[serde(rename = "token_address")]
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub twitter: String,
    pub telegram: String,
    pub website: String,
    pub is_listing: bool,
    pub created_at: i64,
    pub transaction_hash: String,
    pub creator: String,
    pub total_supply: String,
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
        info!("get_token_metadata(token_id: {}) completed in {:?}", token_id, elapsed);
        Ok(response)
    }

    async fn fetch_token_metadata(&self, token_id: &str) -> Result<TokenMetadataResponse> {
        #[derive(sqlx::FromRow)]
        struct TokenMetadataRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            description: Option<String>,
            twitter: Option<String>,
            telegram: Option<String>,
            website: Option<String>,
            is_listing: bool,
            created_at: i64,
            transaction_hash: String,
            creator: String,
            total_supply: BigDecimal,
        }

        let row = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, TokenMetadataRow>(
                "SELECT token_id, name, symbol, image_uri, description, twitter, telegram, website, is_listing, created_at, transaction_hash, creator, total_supply FROM token WHERE token_id = $1",
            )
            .bind(token_id)
            .fetch_one(&*self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let token = TokenMetadata {
            token_id: row.token_id,
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description.unwrap_or_default(),
            twitter: row.twitter.unwrap_or_default(),
            telegram: row.telegram.unwrap_or_default(),
            website: row.website.unwrap_or_default(),
            is_listing: row.is_listing,
            created_at: row.created_at,
            transaction_hash: row.transaction_hash,
            creator: row.creator,
            total_supply: row.total_supply.to_string(),
        };

        Ok(TokenMetadataResponse {
            token_metadata: token,
        })
    }
}
