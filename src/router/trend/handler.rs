use axum::{Extension, Json, extract::State};
use tracing::instrument;

use super::path::TrendPath;
use crate::{
    controllers::trend::TrendController,
    result::{AppError, AppJsonResult},
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
    let controller = TrendController::new(state.postgres.clone());
    let response = controller.get_trend_tokens().await?;

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
    let controller = TrendController::new(state.postgres.clone());

    // Check if user is admin
    let is_admin = controller.is_admin(&session_address).await?;
    if !is_admin {
        return Err(AppError::AuthError("Admin access required".to_string()));
    }

    let response = controller.insert_trend_token(payload).await?;

    Ok(Json(response))
}

/// Delete trend token (Admin only)
#[utoipa::path(
    delete,
    path = TrendPath::DeleteTrend.docs_str(),
    request_body = TrendRequest,
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Trend token deleted successfully", body = TrendActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Trend"
)]
#[instrument(skip(state, payload))]
pub async fn delete_trend(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<TrendRequest>,
) -> AppJsonResult<TrendActionResponse> {
    let controller = TrendController::new(state.postgres.clone());

    // Check if user is admin
    let is_admin = controller.is_admin(&session_address).await?;
    if !is_admin {
        return Err(AppError::AuthError("Admin access required".to_string()));
    }

    let response = controller.delete_trend_token(payload).await?;

    Ok(Json(response))
}
