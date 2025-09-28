use axum::{
    Json,
    extract::{Path, State},
};

use tracing::instrument;

use super::path::TokenPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::token::{detail::TokenService, metadata::TokenMetadataService},
    state::AppState,
    types::token::{TokenResponse, metadata::TokenMetadataResponse},
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
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    let service = TokenService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_token(&token_id).await?;

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
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    let service = TokenMetadataService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_token_metadata(&token_address).await?;

    Ok(Json(response))
}
