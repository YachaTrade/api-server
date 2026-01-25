use axum::{
    Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::LeaderboardPath;
use crate::{
    result::AppJsonResult,
    services::leaderboard::LeaderboardService,
    state::AppState,
    types::leaderboard::{HypePointLeaderboardResponse, LeaderboardQuery, PnlLeaderboardResponse},
};

/// Get hype point leaderboard
#[utoipa::path(
    get,
    path = LeaderboardPath::GetHypePointLeaderboard.docs_str(),
    params(LeaderboardQuery),
    responses(
        (status = 200, description = "Hype point leaderboard fetched successfully", body = HypePointLeaderboardResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "Leaderboard"
)]
#[instrument(skip(state))]
pub async fn get_hype_point_leaderboard(
    State(state): State<AppState>,
    Query(query): Query<LeaderboardQuery>,
) -> AppJsonResult<HypePointLeaderboardResponse> {
    let service = LeaderboardService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_hype_point_leaderboard(&query).await?;

    Ok(Json(response))
}

/// Get PnL leaderboard
#[utoipa::path(
    get,
    path = LeaderboardPath::GetPnlLeaderboard.docs_str(),
    params(LeaderboardQuery),
    responses(
        (status = 200, description = "PnL leaderboard fetched successfully", body = PnlLeaderboardResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "Leaderboard"
)]
#[instrument(skip(state))]
pub async fn get_pnl_leaderboard(
    State(state): State<AppState>,
    Query(query): Query<LeaderboardQuery>,
) -> AppJsonResult<PnlLeaderboardResponse> {
    let service = LeaderboardService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_pnl_leaderboard(&query).await?;

    Ok(Json(response))
}
