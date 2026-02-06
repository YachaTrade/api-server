use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Chest level thresholds in USD (hardcoded)
pub fn chest_level_threshold() -> Value {
    serde_json::json!({
        "1": "100",
        "2": "500",
        "3": "1000",
        "4": "5000",
        "5": "10000"
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
    /// Raw token amount
    pub amount: String,
    /// USD value of the reward
    pub usd_value: String,
}

/// Rewards response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChesterRewardsResponse {
    /// List of reward items with USD values
    pub rewards: Vec<ChesterRewardItem>,
}
