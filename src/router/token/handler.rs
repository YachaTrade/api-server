use axum::{
    extract::{Path, State},
    Json,
};

use tracing::{error, info, instrument};

use super::path::TokenPath;
use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::token::{
        metadata::{TokenMetadataController, TokenMetadataResponse},
        TokenController, TokenResponse,
    },
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
#[instrument(skip(state))]
pub async fn get_token(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> AppJsonResult<TokenResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    if let Ok(cached_response) = state.trade_redis.get_token_response(&token_id).await {
        return Ok(Json(cached_response));
    }
    info!("Get token Request for token: {}", token_id);
    let token_controller = TokenController::new(state.postgres.clone());
    let response = token_controller.get_token(&token_id).await.map_err(|err| {
        error!(
            "Failed to get token: token_id: {}, error: {}",
            token_id, err
        );
        AppError::NotFound(err.to_string())
    })?;
    if let Err(e) = state
        .trade_redis
        .set_token_response(&token_id, &response)
        .await
    {
        error!("Failed to set token response: {}", e);
    }
    info!(
        "Get Token: token_id: {}, response: {:?}",
        token_id, response
    );

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = TokenPath::GetMetadata.docs_str(),
    params(
        ("token" = String, Path, description = "Token address")
    ),
    responses(
        (status = 200, description = "Token metadata fetched successfully", body = TokenMetadataResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Token"
)]
#[instrument(skip(state))]
pub async fn get_token_metadata(
    State(state): State<AppState>,
    Path(token_address): Path<String>,
) -> AppJsonResult<TokenMetadataResponse> {
    if !valid_evm_address(&token_address) {
        error!("Invalid token ID format: {}", token_address);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    if let Ok(cached_response) = state.trade_redis.get_token_metadata(&token_address).await {
        return Ok(Json(cached_response));
    }
    let token_metadata_controller = TokenMetadataController::new(state.postgres.clone());
    let response = token_metadata_controller
        .get_token_metadata(&token_address)
        .await
        .map_err(|err| {
            error!(
                "Failed to get token metadata: token_address: {}, error: {}",
                token_address, err
            );
            AppError::InternalError(err.to_string())
        })?;
    if let Err(e) = state
        .trade_redis
        .set_token_metadata(&token_address, &response)
        .await
    {
        error!("Failed to set token metadata: {}", e);
    }
    info!(
        "Get Token Metadata: token_address: {}, response: {:?}",
        token_address, response
    );

    Ok(Json(response))
}
