use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::path::TokenPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::token::{detail::TokenService, metadata::TokenMetadataService, salt::SaltService},
    state::AppState,
    types::token::{
        TokenResponse,
        metadata::TokenMetadataResponse,
        salt::{MineSaltRequest, MineSaltResponse},
    },
    utils::valid_token_id,
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
    let token_id = valid_token_id(&token_id)
        .ok_or_else(|| AppError::BadRequest("Invalid token ID".to_string()))?;

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
    let token_address = valid_token_id(&token_address)
        .ok_or_else(|| AppError::BadRequest("Invalid token ID".to_string()))?;

    let service = TokenMetadataService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_token_metadata(&token_address).await?;

    Ok(Json(response))
}

/// Mine a salt value to generate a token address ending with a specific suffix
#[utoipa::path(
    post,
    path = TokenPath::Salt.docs_str(),
    request_body = MineSaltRequest,
    responses(
        (status = 200, description = "Salt mined successfully", body = MineSaltResponse),
        (status = 400, description = "Bad request - invalid parameters"),
        (status = 408, description = "Request timeout - max iterations reached"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Token"
)]
#[instrument(skip(_state))]
pub async fn salt(
    State(_state): State<AppState>,
    Json(payload): Json<MineSaltRequest>,
) -> AppJsonResult<MineSaltResponse> {
    payload.validate().map_err(AppError::BadRequest)?;

    let service = SaltService::new();
    let response = service.mine_salt(payload).await?;

    Ok(Json(response))
}
