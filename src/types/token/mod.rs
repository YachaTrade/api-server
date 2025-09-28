pub mod create_token;

pub mod metadata;
pub mod order;
use super::common::info::AccountInfo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, Serialize, sqlx::FromRow, ToSchema)]
pub struct TokenWithAccountInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub is_listing: bool,
    pub created_at: i64,
    pub transaction_hash: String,
    pub account_info: AccountInfo,
    pub is_king: bool,
    pub is_king_created_at: Option<i64>,
    pub total_supply: String,
    pub price: String,
    pub market_cap: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenResponse {
    pub token: TokenWithAccountInfo,
}
