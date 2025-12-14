use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetNsfwRequest {
    pub token_id: String,
    pub is_nsfw: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CmsActionResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InsertTrendRequest {
    pub token_ids: Vec<String>,
}
