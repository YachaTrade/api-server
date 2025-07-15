use axum::{
    extract::{Query, State},
    Json,
};

use tracing::{error, info, instrument};

use super::path::HypePath;
use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        token::hype::{HypeTokenController, HypeTokenResponse},
    },
};

/// Get Hype Token
#[utoipa::path(
    get,
    path = HypePath::GetHype.docs_str(),
    responses(
        (status = 200, description = "Hype Token fetched successfully", body = HypeTokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_hype_token(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationParams>,
) -> AppJsonResult<HypeTokenResponse> {
    info!("Get Hype Token: pagination: {:?}", pagination);
    if let Ok(cached_response) = state.redis.get_hype_token_response(&pagination).await {
        return Ok(Json(cached_response));
    }
    info!("Get Hype Token: pagination: {:?}, cache miss", pagination);
    let hype_token_controller = HypeTokenController::new(state.postgres.clone());
    let response = hype_token_controller
        .get_hype_token(&pagination)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype token, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_token_response(&pagination, &response)
        .await
    {
        error!("Failed to set hype token response: {}", e);
    }
    info!(
        "Get Hype Token: pagination: {:?}, response: {:?}",
        pagination, response
    );

    Ok(Json(response))
}
