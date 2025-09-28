use super::common::info::{AccountInfo, TokenInfo};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchToken {
    pub token_info: TokenInfo,
    pub price: String,
    pub market_cap: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchTokenResponse {
    pub tokens: Vec<SearchToken>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccount {
    pub account_info: AccountInfo,
    pub total_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccountResponse {
    pub accounts: Vec<SearchAccount>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub tokens: SearchTokenResponse,
    pub accounts: SearchAccountResponse,
}
