use axum::{Extension, Json, extract::State};
use tracing::instrument;

use super::path::CmsPath;
use crate::{
    result::AppJsonResult,
    services::cms::CmsService,
    state::AppState,
    types::cms::{CmsActionResponse, InsertTrendRequest, SetNsfwRequest},
};

/// Set token NSFW status (Admin only)
#[utoipa::path(
    post,
    path = CmsPath::SetNsfw.docs_str(),
    request_body = SetNsfwRequest,
    responses(
        (status = 200, description = "NSFW status updated successfully", body = CmsActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, payload))]
pub async fn set_nsfw(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<SetNsfwRequest>,
) -> AppJsonResult<CmsActionResponse> {
    let service = CmsService::new(state.postgres.clone(), state.redis.clone());
    let response = service.set_nsfw(&session_address, payload).await?;

    Ok(Json(response))
}

/// Insert trend tokens (Admin only)
#[utoipa::path(
    post,
    path = CmsPath::InsertTrend.docs_str(),
    request_body = InsertTrendRequest,
    responses(
        (status = 200, description = "Trend tokens inserted successfully", body = CmsActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, payload))]
pub async fn insert_trend(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<InsertTrendRequest>,
) -> AppJsonResult<CmsActionResponse> {
    let service = CmsService::new(state.postgres.clone(), state.redis.clone());
    let response = service.insert_trend(&session_address, payload).await?;

    Ok(Json(response))
}
