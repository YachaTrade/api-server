use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::utils::valid_evm_address;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetNsfwRequest {
    pub token_id: String,
    pub is_nsfw: bool,
}

impl SetNsfwRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_evm_address(&self.token_id) {
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
            if !valid_evm_address(token_id) {
                return Err(format!("Invalid token_id: {}", token_id));
            }
        }
        Ok(())
    }
}
