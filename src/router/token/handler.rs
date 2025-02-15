use axum::{
    extract::{Path, Query, State},
    Json,
};

use tracing::{error, info, instrument};

use super::path::TokenPath;
use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::token::{TokenController, TokenResponse},
    utils::valid_evm_address,
};

/// Get token metadata
#[utoipa::path(
    get,
    path = TokenPath::GetToken.docs_str(),
    params(
        ("token" = String, Path, description = "Token address")
    ),
    responses(
        (status = 200, description = "Token fetched successfully", body = TokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Token"
)]
#[instrument(skip_all)]
pub async fn get_token(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> AppJsonResult<TokenResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    info!("Get token Request for token: {}", token_id);
    let token_controller = TokenController::new(state.postgres.clone());
    let response = token_controller.get_token(&token_id).await.map_err(|err| {
        error!(
            "Failed to get token: token_id: {}, error: {}",
            token_id, err
        );
        AppError::InternalError(err.to_string())
    })?;
    info!(
        "Get Token: token_id: {}, response: {:?}",
        token_id, response
    );
    Ok(Json(response))
}
