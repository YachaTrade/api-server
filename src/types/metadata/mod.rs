use crate::result::AppError;
use serde::{Deserialize, Serialize};
use std::env;
use utoipa::ToSchema;

// Validation constants
pub const MIN_NAME_LENGTH: usize = 1;
pub const MAX_NAME_LENGTH: usize = 32;
pub const MIN_SYMBOL_LENGTH: usize = 1;
pub const MAX_SYMBOL_LENGTH: usize = 10;
pub const MAX_DESCRIPTION_LENGTH: usize = 500;

#[derive(ToSchema)]
pub struct UploadImageMultipart {
    /// Image file to upload (JPEG, PNG, WebP, SVG)
    #[schema(value_type = String, format = Binary)]
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadImageResponse {
    pub is_nsfw: bool,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadMetadataRequest {
    pub image_uri: String,
    pub name: String,
    pub symbol: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UploadMetadataResponse {
    pub metadata_uri: String,
    pub metadata: TokenMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub description: Option<String>,
    pub image_uri: String,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub is_nsfw: bool,
}

impl TokenMetadata {
    /// Validate all metadata fields
    pub fn validate(&self) -> Result<(), AppError> {
        // Validate name: 1-32 chars, no newlines
        let name = self.name.trim();
        if name.len() < MIN_NAME_LENGTH {
            return Err(AppError::BadRequest(format!(
                "Token name must be at least {} character",
                MIN_NAME_LENGTH
            )));
        }
        if name.len() > MAX_NAME_LENGTH {
            return Err(AppError::BadRequest(format!(
                "Token name must be at most {} characters",
                MAX_NAME_LENGTH
            )));
        }
        if name.contains('\n') || name.contains('\r') {
            return Err(AppError::BadRequest(
                "Token name cannot contain newlines".to_string(),
            ));
        }

        // Validate symbol: 1-10 chars, alphanumeric only
        let symbol = self.symbol.trim();
        if symbol.len() < MIN_SYMBOL_LENGTH {
            return Err(AppError::BadRequest(format!(
                "Token symbol must be at least {} character",
                MIN_SYMBOL_LENGTH
            )));
        }
        if symbol.len() > MAX_SYMBOL_LENGTH {
            return Err(AppError::BadRequest(format!(
                "Token symbol must be at most {} characters",
                MAX_SYMBOL_LENGTH
            )));
        }
        if !symbol.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::BadRequest(
                "Token symbol must contain only letters and numbers".to_string(),
            ));
        }

        // Validate description: max 500 chars (optional)
        if let Some(ref desc) = self.description
            && desc.len() > MAX_DESCRIPTION_LENGTH
        {
            return Err(AppError::BadRequest(format!(
                "Description must be at most {} characters",
                MAX_DESCRIPTION_LENGTH
            )));
        }

        // Validate image_uri is not empty
        if self.image_uri.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Image URI cannot be empty".to_string(),
            ));
        }

        // Validate image URL domain
        let allowed_image_domain = env::var("ALLOWED_IMAGE_DOMAIN")
            .unwrap_or_else(|_| "https://storage.nadapp.net/".to_string());

        if !self.image_uri.starts_with(&allowed_image_domain) {
            return Err(AppError::BadRequest(format!(
                "Invalid image URI - must be from {}",
                allowed_image_domain
            )));
        }

        // Validate website URL - must start with https://
        if let Some(website_url) = &self.website
            && !website_url.is_empty()
            && !website_url.starts_with("https://")
        {
            return Err(AppError::BadRequest(
                "Invalid website URL - must start with https://".to_string(),
            ));
        }

        // Validate Twitter URL - must start with https://x.com/
        if let Some(twitter_url) = &self.twitter
            && !twitter_url.is_empty()
            && !twitter_url.starts_with("https://x.com/")
        {
            return Err(AppError::BadRequest(
                "Invalid X (Twitter) URL - must start with https://x.com/".to_string(),
            ));
        }

        // Validate Telegram URL - must start with https://t.me/
        if let Some(telegram_url) = &self.telegram
            && !telegram_url.is_empty()
            && !telegram_url.starts_with("https://t.me/")
        {
            return Err(AppError::BadRequest(
                "Invalid Telegram URL - must start with https://t.me/".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TerminalMetadataResponse {
    pub image: String,
    pub description: String,
    pub website: String,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}
