use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};

use tracing::instrument;

use crate::{
    result::AppJsonResult,
    services::chester::ChesterService,
    state::AppState,
    types::{
        chester::{
            ChesterBoxRewardsQuery, ChesterBoxRewardsResponse, ChesterInfoResponse,
            ChesterRewardsResponse, ChesterVolumeResponse,
        },
        common::pagination::PaginationParams,
        profile::SwapHistoryResponse,
    },
};

use super::path::ChesterPath;

/// Get cumulative USD trading volume for an account in the active round
#[utoipa::path(
    get,
    path = ChesterPath::Volume.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account address")
    ),
    responses(
        (status = 200, description = "Account trading volume", body = ChesterVolumeResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_volume(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> AppJsonResult<ChesterVolumeResponse> {
    let service = ChesterService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_volume(&account_id).await?;

    Ok(Json(response))
}

/// Get current active round information
#[utoipa::path(
    get,
    path = ChesterPath::Round.docs_str(),
    responses(
        (status = 200, description = "Current round information", body = ChesterInfoResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_round(
    State(state): State<AppState>,
) -> AppJsonResult<Option<ChesterInfoResponse>> {
    let service = ChesterService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_round().await?;

    Ok(Json(response))
}

/// Get reward items with USD values for the active round
#[utoipa::path(
    get,
    path = ChesterPath::Rewards.docs_str(),
    responses(
        (status = 200, description = "Round rewards with USD values", body = ChesterRewardsResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_rewards(
    State(state): State<AppState>,
) -> AppJsonResult<ChesterRewardsResponse> {
    let service = ChesterService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_rewards().await?;

    Ok(Json(response))
}

/// Get box rewards for the authenticated account
#[utoipa::path(
    get,
    path = ChesterPath::BoxRewards.docs_str(),
    params(
        ("round" = Option<i64>, Query, description = "Round number (defaults to latest round)")
    ),
    responses(
        (status = 200, description = "Box rewards for the account", body = ChesterBoxRewardsResponse),
        (status = 401, description = "Unauthorized - session required")
    ),
    tag = "Chester"
)]
#[instrument(skip(state, session_address))]
pub async fn get_box_rewards(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<ChesterBoxRewardsQuery>,
) -> AppJsonResult<ChesterBoxRewardsResponse> {
    let service = ChesterService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_box_rewards(&session_address, query.round).await?;

    Ok(Json(response))
}

/// Get swap history for an account within the active chester round
#[utoipa::path(
    get,
    path = ChesterPath::SwapHistory.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account address"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("limit" = Option<i64>, Query, description = "Items per page (default: 10, max: 100)")
    ),
    responses(
        (status = 200, description = "Swap history within active round", body = SwapHistoryResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_swap_history(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<SwapHistoryResponse> {
    let service = ChesterService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_swap_history(&account_id, params.page, params.limit)
        .await?;

    Ok(Json(response))
}
