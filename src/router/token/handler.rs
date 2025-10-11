use axum::{
    Json,
    extract::{Path, State},
};

use tracing::instrument;
use std::str::FromStr;
use std::time::Instant;
use alloy::primitives::Address;
use uuid::Uuid;

use super::path::TokenPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::token::{detail::TokenService, metadata::TokenMetadataService},
    state::AppState,
    types::token::{TokenResponse, metadata::TokenMetadataResponse, mine_salt::{MineSaltRequest, MineSaltResponse}},
    utils::{valid_evm_address, create2},
};

use tracing::info;

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
    path = TokenPath::MineSalt.docs_str(),
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
    // Validate creator address
    if !valid_evm_address(&payload.creator) {
        return Err(AppError::BadRequest(format!(
            "Invalid creator address: {}",
            payload.creator
        )));
    }

    // Get deployer, implementation, and suffix from environment variables
    let deployer_str = std::env::var("BONDING_CURVE")
        .map_err(|_| AppError::InternalError("BONDING_CURVE environment variable not set".to_string()))?;

    let implementation_str = std::env::var("TOKEN_IMPLEMENT")
        .map_err(|_| AppError::InternalError("TOKEN_IMPLEMENT environment variable not set".to_string()))?;

    let suffix = std::env::var("VANITY_ADDRESS_SUFFIX")
        .map_err(|_| AppError::InternalError("VANITY_ADDRESS_SUFFIX environment variable not set".to_string()))?;

    // Validate addresses from env
    if !valid_evm_address(&deployer_str) {
        return Err(AppError::InternalError(format!(
            "Invalid BONDING_CURVE address in environment: {}",
            deployer_str
        )));
    }

    if !valid_evm_address(&implementation_str) {
        return Err(AppError::InternalError(format!(
            "Invalid TOKEN_IMPLEMENT address in environment: {}",
            implementation_str
        )));
    }

    // Validate suffix is valid hex
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::InternalError(format!(
            "Invalid VANITY_ADDRESS_SUFFIX in environment: must be non-empty hex string (got '{}')",
            suffix
        )));
    }

    // Parse addresses
    let deployer = Address::from_str(&deployer_str).map_err(|e| {
        AppError::InternalError(format!("Failed to parse BONDING_CURVE address: {}", e))
    })?;

    let implementation = Address::from_str(&implementation_str).map_err(|e| {
        AppError::InternalError(format!("Failed to parse TOKEN_IMPLEMENT address: {}", e))
    })?;

    // Set reasonable default max iterations (10 million)
    let max_iterations = Some(10_000_000u64);

    info!(
        "Starting salt mining: creator={}, name={}, symbol={}, deployer={}, implementation={}, suffix='{}'",
        payload.creator, payload.name, payload.symbol, deployer_str, implementation_str, suffix
    );

    // Start mining
    let start_time = Instant::now();

    // Generate unique UUID for this mining request
    let request_uuid = Uuid::new_v4().to_string();

    // Clone data for the closure
    let suffix_clone = suffix.clone();
    let creator_clone = payload.creator.clone();
    let name_clone = payload.name.clone();
    let symbol_clone = payload.symbol.clone();
    let token_uri_clone = payload.token_uri.clone();
    let uuid_clone = request_uuid.clone();

    info!("Generated UUID for mining request: {}", request_uuid);

    // Run mining in a blocking task to avoid blocking the async runtime
    let result = tokio::task::spawn_blocking(move || {
        create2::mine_salt_with_suffix(
            deployer,
            implementation,
            &suffix_clone,
            max_iterations,
            &creator_clone,
            &name_clone,
            &symbol_clone,
            &token_uri_clone,
            &uuid_clone,
        )
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Mining task failed: {}", e)))?;

    let mining_time = start_time.elapsed();

    // Check if we found a result
    match result {
        Some(mined) => {
            info!(
                "✓ Salt mined successfully in {} iterations! salt={}, address={}, time={}ms",
                mined.iterations,
                hex::encode(mined.salt.as_slice()),
                mined.address,
                mining_time.as_millis()
            );

            Ok(Json(MineSaltResponse {
                salt: format!("0x{}", hex::encode(mined.salt.as_slice())),
                address: mined.address.to_string(),
            }))
        }
        None => {
            info!(
                "Salt mining failed: no match found after {:?} iterations in {}ms",
                max_iterations,
                mining_time.as_millis()
            );

            Err(AppError::InternalError(format!(
                "Failed to find salt with suffix '{}' after {} iterations ({}ms).",
                suffix,
                max_iterations.unwrap_or(0),
                mining_time.as_millis()
            )))
        }
    }
}
