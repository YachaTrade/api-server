use axum::{Json, extract::{Query, State}};
use tracing::instrument;

use super::path::LeaderboardPath;
use crate::{
    result::AppJsonResult,
    services::leaderboard::LeaderboardService,
    state::AppState,
    types::leaderboard::{HypePointLeaderboardResponse, LeaderboardQuery},
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
