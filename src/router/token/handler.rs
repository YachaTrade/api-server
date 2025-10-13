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
        mine_salt::{MineSaltRequest, MineSaltResponse},
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

/// Mine a salt value to generate a token address ending with a specific suffix
///
/// This endpoint uses CREATE2 address calculation with EIP-1167 minimal proxy pattern
/// to find a salt value that produces a token address ending with the desired suffix.
/// The mining process runs in parallel for optimal performance.
///
/// Uses environment variables:
/// - BONDING_CURVE: deployer/factory contract address
/// - TOKEN_IMPLEMENT: implementation contract address
/// - VANITY_ADDRESS_SUFFIX: desired suffix (e.g., "143")
///
/// # Example
/// For VANITY_ADDRESS_SUFFIX="143", this will find a salt that produces an address like:
/// `0x742d35Cc6634C0532925a3b844Bc9e7595f0143`
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
pub async fn mine_salt(
    State(_state): State<AppState>,
    Json(payload): Json<MineSaltRequest>,
) -> AppJsonResult<MineSaltResponse> {
    let service = SaltService::new();
    let response = service.mine_salt(payload).await?;

    Ok(Json(response))
}
