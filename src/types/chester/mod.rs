use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Chest level thresholds in USD (hardcoded)
pub fn chest_level_threshold() -> Value {
    serde_json::json!({
        "1": "1000",
        "2": "6000",
        "3": "16000",
        "4": "116000",
    })
}

/// Account volume response for chester round.
/// `total_usd_volume` is returned as String to preserve NUMERIC precision.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterVolumeResponse {
    /// Cumulative USD trading volume for the active round
    pub volume_usd: String,
    /// Cumulative USD fee for the active round
    pub fee_usd: String,
}

/// Round info response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterInfoResponse {
    /// Round number
    pub round: i64,
    /// Round start time (epoch seconds)
    pub start_at: i64,
    /// Round end time (epoch seconds)
    pub end_at: i64,
    /// Round status (ACTIVE, COMPLETED, READY)
    pub status: String,
    /// Chest level thresholds in USD
    pub chest_level_threshold: Value,
}

/// Reward item with USD value
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterRewardItem {
    /// Token contract address
    pub token_id: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Token image URI
    pub image_uri: String,
    /// Raw token amount
    pub amount: String,
    /// Token price in USD
    pub price: String,
    /// USD value of the reward
    pub usd_value: String,
}

/// Rewards response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterRewardsResponse {
    /// List of reward items with USD values
    pub rewards: Vec<ChesterRewardItem>,
}

/// Box reward item for a user
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterBoxRewardItem {
    /// Round number
    pub round: i64,
    /// Box level (1~4)
    pub level: i32,
    /// Token contract address
    pub token_id: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Token image URI
    pub image_uri: String,
    /// Raw token amount
    pub amount: String,
    /// Reward status (AWAITING | CLAIMED)
    pub status: String,
    /// Merkle proof
    pub proof: Vec<String>,
    /// Claim transaction hash
    pub transaction_hash: Option<String>,
    /// Claimed timestamp (epoch seconds)
    pub claimed_at: Option<i64>,
}

/// Box rewards response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterBoxRewardsResponse {
    /// List of box reward items
    pub rewards: Vec<ChesterBoxRewardItem>,
}

/// Query params for box rewards
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterBoxRewardsQuery {
    /// Round number (optional, defaults to latest round)
    pub round: Option<i64>,
}

/// Single flat reward history item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RewardHistoryItem {
    /// Round number
    pub round: i64,
    /// Box level (1~4)
    pub level: i32,
    /// Token contract address or "hype" for CHEST points
    pub token_id: String,
    /// Raw token amount or point amount
    pub amount: String,
    /// Claim transaction hash (null for hype points)
    pub transaction_hash: Option<String>,
    /// Claimed timestamp (epoch seconds)
    pub created_at: i64,
}

/// Reward history response with pagination
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterRewardHistoryResponse {
    /// List of reward history items
    pub histories: Vec<RewardHistoryItem>,
    /// Total number of distinct claim events
    pub total_count: i64,
}

/// Round history response with pagination
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterHistoryResponse {
    /// Total number of rounds
    pub total_count: i64,
    /// List of rounds
    pub rounds: Vec<ChesterHistoryItem>,
}

/// Single round history item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterHistoryItem {
    /// Round number
    pub round: i64,
    /// Round start time (epoch seconds)
    pub start_at: i64,
    /// Round end time (epoch seconds)
    pub end_at: i64,
    /// Round status (ACTIVE, COMPLETED, READY)
    pub status: String,
}
