use super::common::info::{AccountInfo, MarketInfo, TokenInfo};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSearchResult {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSearchResponse {
    pub tokens: Vec<TokenSearchResult>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountSearchResult {
    pub account_info: AccountInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountSearchResponse {
    pub accounts: Vec<AccountSearchResult>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub account_result: AccountSearchResponse,
    pub token_result: TokenSearchResponse,
}
