use axum::{extract::State, response::Json};
use tracing::{error, warn};

use crate::{
    result::{AppError, AppJsonResult},
    router::new_content::path::NewContentPath,
    state::AppState,
    types::new_content::{NewContentController, NewContentResponse},
};

/// Get latest new content (buy/sell/token)
#[utoipa::path(
    get,
    path = NewContentPath::NewContent.docs_str(),
    responses(
        (status = 200, description = "Latest new content retrieved successfully", body = NewContentResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "New Content"
)]
pub async fn get_new_content(State(state): State<AppState>) -> AppJsonResult<NewContentResponse> {
    // Try Redis cache first
    if let Ok(cached_response) = state.redis.get_new_content().await {
        return Ok(Json(cached_response));
    }

    let controller = NewContentController::new(state.postgres.clone());

    let response = controller.get_new_content().await.map_err(|err| {
        error!("Failed to get new content: {}", err);
        AppError::InternalError(err.to_string())
    })?;

    // Cache the result for 1 second
    if let Err(err) = state.redis.set_new_content(&response).await {
        warn!("Failed to set new content cache: {}", err);
    }

    Ok(Json(response))
}
