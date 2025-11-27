use axum::{Extension, Json, extract::State};
use tracing::instrument;

use super::path::TrendPath;
use crate::{
    result::AppJsonResult,
    services::trend::TrendService,
    state::AppState,
    types::trend::{TrendActionResponse, TrendRequest, TrendResponse},
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

/// Insert trend token (Admin only)
#[utoipa::path(
    post,
    path = TrendPath::InsertTrend.docs_str(),
    request_body = TrendRequest,
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Trend token inserted successfully", body = TrendActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Trend"
)]
#[instrument(skip(state, payload))]
pub async fn insert_trend(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<TrendRequest>,
) -> AppJsonResult<TrendActionResponse> {
    let service = TrendService::new(state.postgres.clone(), state.redis.clone());
    let response = service.insert_trend(&session_address, payload).await?;

    Ok(Json(response))
}

