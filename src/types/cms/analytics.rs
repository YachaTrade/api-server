use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ── Query params ──

#[derive(Debug, Deserialize, IntoParams)]
pub struct UserActivityQuery {
    /// 마지막 swap 후 N일 경과 시 이탈 판단
    pub inactive_days: i64,
    /// top 보유 토큰 개수 (default 10)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct NewUsersQuery {
    /// 최근 N일간 신규 유저 조회
    pub days: i64,
}

// ── Response types ──

#[derive(Debug, Serialize, ToSchema)]
pub struct TopHeldToken {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub holder_count: i64,
    pub avg_balance: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserActivityResponse {
    pub total_users: i64,
    pub avg_pnl_usd: String,
    pub avg_pnl_native: String,
    pub avg_volume_usd: String,
    pub avg_volume_native: String,
    pub top_held_tokens: Vec<TopHeldToken>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NewUsersResponse {
    pub new_users: i64,
    pub period_days: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserRoiResponse {
    pub avg_roi_percent: String,
    pub median_roi_percent: String,
    pub positive_roi_count: i64,
    pub negative_roi_count: i64,
    pub total_users: i64,
}
