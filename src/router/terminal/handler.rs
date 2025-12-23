use axum::{
    Json,
    extract::{Path, Query, State},
};
use tracing::{info, instrument};

use super::path::TerminalPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::{terminal::TerminalService, metadata::MetadataService},
    state::AppState,
    types::{terminal::*, metadata::TerminalMetadataResponse},
    utils::valid_evm_address,
};

/// Get latest block
#[utoipa::path(
    get,
    path = "/latest-block",
    responses(
        (status = 200, description = "Latest block fetched successfully", body = LatestBlockResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "Terminal"
)]
#[instrument(skip(state))]
pub async fn get_latest_block(
    State(state): State<AppState>,
) -> AppJsonResult<LatestBlockResponse> {
    let service = TerminalService::new(state.postgres.clone());
    let response = service.get_latest_block().await?;

    Ok(Json(response))
}

/// Get asset information
#[utoipa::path(
    get,
    path = "/asset",
    params(
        ("id" = String, Query, description = "Asset ID")
    ),
    responses(
        (status = 200, description = "Asset fetched successfully", body = AssetResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Terminal"
)]
#[instrument(skip(state))]
pub async fn get_asset(
    State(state): State<AppState>,
    Query(query): Query<AssetQuery>,
) -> AppJsonResult<AssetResponse> {
    let service = TerminalService::new(state.postgres.clone());
    let response = service.get_asset(&query.id).await?;

    Ok(Json(response))
}

/// Get pair information
#[utoipa::path(
    get,
    path = "/pair",
    params(
        ("id" = String, Query, description = "Pair ID")
    ),
    responses(
        (status = 200, description = "Pair fetched successfully", body = PairResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Terminal"
)]
#[instrument(skip(state))]
pub async fn get_pair(
    State(state): State<AppState>,
    Query(query): Query<PairQuery>,
) -> AppJsonResult<PairResponse> {
    let service = TerminalService::new(state.postgres.clone());
    let response = service.get_pair(&query.id).await?;

    Ok(Json(response))
}

/// Get events for block range
#[utoipa::path(
    get,
    path = "/events",
    params(
        ("fromBlock" = u64, Query, description = "Starting block number (inclusive)"),
        ("toBlock" = u64, Query, description = "Ending block number (inclusive)")
    ),
    responses(
        (status = 200, description = "Events fetched successfully", body = EventsResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Terminal"
)]
#[instrument(skip(state))]
pub async fn get_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> AppJsonResult<EventsResponse> {
    query.validate().map_err(AppError::BadRequest)?;

    let service = TerminalService::new(state.postgres.clone());
    let response = service.get_events(query.from_block, query.to_block).await?;

    Ok(Json(response))
}

/// Get terminal metadata for token
#[utoipa::path(
    get,
    path = TerminalPath::GetMetadata.docs_str(),
    params(
        ("token_address" = String, Path, description = "Token contract address")
    ),
    responses(
        (status = 200, description = "Terminal metadata retrieved successfully", body = TerminalMetadataResponse),
        (status = 404, description = "Token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Terminal"
)]
#[instrument(skip(state))]
pub async fn get_terminal_metadata(
    State(state): State<AppState>,
    Path(token_address): Path<String>,
) -> AppJsonResult<TerminalMetadataResponse> {
    if !valid_evm_address(&token_address) {
        return Err(AppError::BadRequest("Invalid token address format".to_string()));
    }

    info!("Getting terminal metadata for token: {}", token_address);

    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );

    let response = service.get_terminal_metadata(&token_address).await?;

    info!("✅ Terminal metadata retrieved for token: {}", token_address);

    Ok(Json(response))
}
