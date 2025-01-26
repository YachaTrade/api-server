use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfo {
    pub token_id: String,
    pub symbol: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub follower_count: i32,
    pub following_count: i32,
}
