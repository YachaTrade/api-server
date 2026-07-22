use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{
    AccountInfo, TokenCreatedInfo, TokenSwapInfo, TokenWithBalanceInfo,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileResponse {
    pub account_info: AccountInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldTokenResponse {
    pub tokens: Vec<TokenWithBalanceInfo>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SwapHistoryResponse {
    pub swaps: Vec<TokenSwapInfo>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedTokensResponse {
    pub tokens: Vec<TokenCreatedInfo>,
    pub total_count: i64,
}
