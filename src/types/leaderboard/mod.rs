use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::types::common::{
    info::AccountInfo,
    pagination::{default_page, deserialize_limit, deserialize_page},
};

fn default_leaderboard_limit() -> i64 {
    10
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
    #[param(default = 1, minimum = 1)]
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    #[param(default = 10, minimum = 1, maximum = 100)]
    #[serde(
        default = "default_leaderboard_limit",
        deserialize_with = "deserialize_limit"
    )]
    pub limit: i64,
}

// PnL Leaderboard Types

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pnl {
    /// 실현 손익 (MON)
    pub realized_native: String,
    /// 미실현 손익 (MON)
    pub unrealized_native: String,
    /// 총 손익 (MON) = realized + unrealized
    pub total_native: String,
    /// 수익률 % (native 기준, total 기준)
    pub native_percent: String,
    /// 실현 손익 (USD)
    pub realized_usd: String,
    /// 미실현 손익 (USD)
    pub unrealized_usd: String,
    /// 총 손익 (USD) = realized + unrealized
    pub total_usd: String,
    /// 수익률 % (usd 기준, total 기준)
    pub usd_percent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PnlLeaderboardEntry {
    pub rank: i64,
    pub account_info: AccountInfo,
    pub pnl: Pnl,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PnlLeaderboardResponse {
    pub ranks: Vec<PnlLeaderboardEntry>,
    pub total_count: i64,
}
