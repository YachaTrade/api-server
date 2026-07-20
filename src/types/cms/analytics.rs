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

#[derive(Debug, Deserialize, IntoParams)]
pub struct CreatorFeeQuery {
    /// 대상 토큰 주소 (EIP-55 체크섬으로 정규화)
    pub token_id: String,
    /// 집계 시작 unix timestamp (초). created_at >= from
    pub from: i64,
    /// 집계 종료 unix timestamp (초, 배타적). created_at < to. 생략 시 상한 없음
    pub to: Option<i64>,
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

/// 토큰 기간별 거래량/크리에이터 수수료 집계.
/// 모든 금액은 해당 토큰의 quote token 단위(raw/10^decimals)로 스케일된 문자열.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatorFeeResponse {
    /// 금액 단위가 되는 quote token 주소 (market.quote_id, 미상 시 native WMON)
    pub quote_id: String,
    /// quote token 심볼 (예: "MON", "USDC")
    pub quote_symbol: String,
    /// quote token decimals (스케일에 사용)
    pub decimals: i32,
    /// 총 거래량 (SUM swap.quote_amount)
    pub total_volume: String,
    /// 순수 creator fee (v1: lp_collect c_amount 합 / v2: "0")
    pub pure_creator_fee: String,
    /// sell fee (v1: fee_distribute creator_amount 합 / v2: "0")
    pub sell_fee: String,
    /// 총 creator fee (v1: pure + sell / v2: creator_fee_distribution amount 합)
    pub total_creator_fee: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserRoiResponse {
    pub avg_roi_percent: String,
    pub median_roi_percent: String,
    pub positive_roi_count: i64,
    pub negative_roi_count: i64,
    pub total_users: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChesterRetentionRound {
    pub from_round: i64,
    pub to_round: i64,
    pub from_participants: i64,
    pub returning_participants: i64,
    pub retention_rate: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChesterRetentionResponse {
    pub rounds: Vec<ChesterRetentionRound>,
}
