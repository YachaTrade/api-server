use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::utils::validate_token_id;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetNsfwRequest {
    pub token_id: String,
    pub is_nsfw: bool,
}

impl SetNsfwRequest {
    pub fn validate(&self) -> Result<(), String> {
        if validate_token_id(&self.token_id).is_none() {
            return Err("Invalid token_id format".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CmsActionResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InsertTrendRequest {
    pub token_ids: Vec<String>,
}

impl InsertTrendRequest {
    const MAX_TREND_TOKENS: usize = 50;

    pub fn validate(&self) -> Result<(), String> {
        if self.token_ids.len() > Self::MAX_TREND_TOKENS {
            return Err(format!(
                "Cannot insert more than {} tokens",
                Self::MAX_TREND_TOKENS
            ));
        }
        for token_id in &self.token_ids {
            if validate_token_id(token_id).is_none() {
                return Err(format!("Invalid token_id: {}", token_id));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateMetadataRequest {
    pub token_id: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}

impl UpdateMetadataRequest {
    pub fn validate(&self) -> Result<(), String> {
        if validate_token_id(&self.token_id).is_none() {
            return Err("Invalid token_id format".to_string());
        }

        // Validate description length
        if let Some(ref desc) = self.description
            && desc.len() > 500
        {
            return Err("Description must be at most 500 characters".to_string());
        }

        // Validate website URL - must start with https://
        if let Some(ref website_url) = self.website
            && !website_url.is_empty()
            && !website_url.starts_with("https://")
        {
            return Err("Invalid website URL - must start with https://".to_string());
        }

        // Validate Twitter URL - must start with https://x.com/
        if let Some(ref twitter_url) = self.twitter
            && !twitter_url.is_empty()
            && !twitter_url.starts_with("https://x.com/")
        {
            return Err("Invalid X (Twitter) URL - must start with https://x.com/".to_string());
        }

        // Validate Telegram URL - must start with https://t.me/
        if let Some(ref telegram_url) = self.telegram
            && !telegram_url.is_empty()
            && !telegram_url.starts_with("https://t.me/")
        {
            return Err("Invalid Telegram URL - must start with https://t.me/".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateMetadataResponse {
    pub success: bool,
    pub metadata_uri: String,
}
