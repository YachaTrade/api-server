use axum::{extract::State, response::Json};

use crate::{
    result::AppJsonResult, router::new_content::path::NewContentPath,
    services::new_content::NewContentService, state::AppState,
    types::new_content::NewContentResponse,
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
    let service = NewContentService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_new_content().await?;
    Ok(Json(response))
}
