use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::TokenInfo;

#[derive(Debug, Deserialize)]
pub struct HypeTokenQuery {
    pub epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeInfo {
    pub vote: String,
    pub holder_count: u64,
    pub market_cap: String,
    pub market_cap_usd: String,
    pub reward_amount: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeToken {
    pub token_info: TokenInfo,
    pub hype_info: HypeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeTokenResponse {
    pub tokens: Vec<HypeToken>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointResponse {
    pub account_id: String,
    pub round_point: String,
    pub hype_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeEpochResponse {
    pub epoch: i64,
    pub start_at: i64,
    pub end_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteHistory {
    pub epoch: i64,
    pub is_live: bool,
    pub token_info: TokenInfo,
    pub vote_amount: String,
    pub reward_amount: String,
    pub claimable: bool,
    pub proof: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteHistoryResponse {
    pub history: Vec<HypeVoteHistory>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointRecord {
    pub epoch: i64,
    pub activity_type: String,
    pub amount: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointRecordResponse {
    pub history: Vec<HypePointRecord>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteRequest {
    pub token_id: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteResponse {
    pub account_id: String,
    pub round_point: String,
    pub hype_point: String,
    pub token_vote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RewardAdd {
    pub epoch: i64,
    pub token_info: TokenInfo,
    pub amount: String,
    pub total_amount: String,
    pub created_at: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeRewardAddHistoryResponse {
    pub history: Vec<RewardAdd>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AmountResponse {
    pub amount: String,
}
