use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::types::common::{
    info::AccountInfo,
    pagination::{deserialize_limit, deserialize_offset},
};

fn default_leaderboard_limit() -> i64 {
    10
}

fn default_offset() -> i64 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointLeaderboardEntry {
    pub rank: i64,
    pub account_info: AccountInfo,
    pub hype_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointLeaderboardResponse {
    pub ranks: Vec<HypePointLeaderboardEntry>,
    pub total_count: i64,
    pub total_hype_point: String,
    pub last_updated_at: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct LeaderboardQuery {
    #[param(default = 10, minimum = 1, maximum = 100)]
    #[serde(
        default = "default_leaderboard_limit",
        deserialize_with = "deserialize_limit"
    )]
    pub limit: i64,
    #[param(default = 0, minimum = 0)]
    #[serde(default = "default_offset", deserialize_with = "deserialize_offset")]
    pub offset: i64,
}
