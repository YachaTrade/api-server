use crate::{db::postgres::PostgresDatabase, result::AppError};
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use std::{env, sync::Arc};
use tracing::{info, warn};
use utoipa::ToSchema;

#[derive(ToSchema)]
pub struct UploadImageMultipart {
    /// Image file to upload (JPEG, PNG, WebP, SVG)
    #[schema(value_type = String, format = Binary)]
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadImageResponse {
    pub is_nsfw: bool,
    pub image_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadMetadataRequest {
    pub image_url: String,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadMetadataResponse {
    pub metadata_url: String,
    pub metadata: TokenMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub image_url: String,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub is_nsfw: bool,
}

impl TokenMetadata {
    /// Validate all metadata fields
    pub fn validate(&self) -> Result<(), AppError> {
        // Validate website URL - HTTPS only for security
        if let Some(website_url) = &self.website {
            if !website_url.is_empty() {
                if !website_url.starts_with("https://") {
                    return Err(AppError::BadRequest(
                        "Invalid website URL format - must start with https://"
                            .to_string(),
                    ));
                }
            }
        }

        // Validate Twitter URL (X) - HTTPS only for security
        if let Some(twitter_url) = &self.twitter {
            if !twitter_url.is_empty() {
                if !twitter_url.contains("x.com") {
                    return Err(AppError::BadRequest(
                        "Invalid X (Twitter) URL format - must contain x.com".to_string(),
                    ));
                }
                if !twitter_url.starts_with("https://") {
                    return Err(AppError::BadRequest(
                        "Invalid X (Twitter) URL format - must start with https://"
                            .to_string(),
                    ));
                }
            }
        }

        // Validate Telegram URL - HTTPS only for security
        if let Some(telegram_url) = &self.telegram {
            if !telegram_url.is_empty() {
                if !telegram_url.contains("t.me") {
                    return Err(AppError::BadRequest(
                        "Invalid Telegram URL format - must contain t.me".to_string(),
                    ));
                }
                if !telegram_url.starts_with("https://") {
                    return Err(AppError::BadRequest(
                        "Invalid Telegram URL format - must start with https://"
                            .to_string(),
                    ));
                }
            }
        }

        // Validate required fields are not empty
        if self.name.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Token name cannot be empty".to_string(),
            ));
        }

        if self.symbol.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Token symbol cannot be empty".to_string(),
            ));
        }

        if self.description.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Token description cannot be empty".to_string(),
            ));
        }

        if self.image_url.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Image URL cannot be empty".to_string(),
            ));
        }

        // Validate image URL domain
        let allowed_image_domain = env::var("ALLOWED_IMAGE_DOMAIN")
            .unwrap_or_else(|_| "https://storage.nadapp.net/".to_string());

        if !self.image_url.starts_with(&allowed_image_domain) {
            return Err(AppError::BadRequest(format!(
                "Invalid image URL - must be from {}",
                allowed_image_domain
            )));
        }

        Ok(())
    }
}

pub struct MetadataController {
    pub db: Arc<PostgresDatabase>,
}

impl MetadataController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetadataController { db }
    }

    pub async fn save_token_metadata(
        &self,
        metadata: &TokenMetadata,
        metadata_url: &str,
    ) -> Result<()> {
        let start_time = Instant::now();

        let query = sqlx::query!(
            r#"
            INSERT INTO token_metadata (metadata_url, name, symbol, description, image_url, website, twitter, telegram, is_nsfw)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            metadata_url,
            metadata.name,
            metadata.symbol,
            metadata.description,
            metadata.image_url,
            metadata.website,
            metadata.twitter,
            metadata.telegram,
            metadata.is_nsfw
        );

        tokio::time::timeout(
            Duration::from_millis(1000),
            query.execute(self.db.get_write_pool()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Query timeout after 1000ms"))?
        .map_err(|err| anyhow::anyhow!("Failed to save token metadata. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        info!(
            "save_token_metadata(name: {}, symbol: {}) completed in {:?}",
            metadata.name, metadata.symbol, elapsed
        );

        if elapsed > Duration::from_millis(100) {
            warn!(
                "save_token_metadata query slow performance: {:?} for name: {}, symbol: {}",
                elapsed, metadata.name, metadata.symbol
            );
        }

        Ok(())
    }
}
