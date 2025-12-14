use axum::{Json, extract::State};
use tracing::instrument;

use super::path::TrendPath;
use crate::{
    result::AppJsonResult,
    services::trend::TrendService,
    state::AppState,
    types::trend::TrendResponse,
};

/// Get trend tokens
#[utoipa::path(
    get,
    path = TrendPath::GetTrend.docs_str(),
    responses(
        (status = 200, description = "Trend tokens fetched successfully", body = TrendResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "Trend"
)]
#[instrument(skip(state))]
pub async fn get_trend(State(state): State<AppState>) -> AppJsonResult<TrendResponse> {
    let service = TrendService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_trend().await?;

    Ok(Json(response))
}

