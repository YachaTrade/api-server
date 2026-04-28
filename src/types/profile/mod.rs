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

/// Tokens where the account is bound as the gift vault receiver
/// (`v2_gift_vault_stats.receiver`). Each token is rendered as a
/// `TokenCreatedInfo` so the UI can reuse the same card layout.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GiftFeeTokensResponse {
    pub tokens: Vec<TokenCreatedInfo>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PointRecord {
    pub epoch: i64,
    pub activity_type: String,
    pub amount: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PointRecordTotal {
    pub total_point: String,
    pub history: Vec<PointRecord>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PointHistoryResponse {
    pub histories: Vec<PointRecordTotal>,
    pub total_count: u64,
}
