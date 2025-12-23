use crate::{
    controllers::account::AccountController,
    result::{AppError, AppJsonResult},
    services::{
        hype::HypeService,
        token::create::TokenCreatedService,
        trading::{position::PositionService, swap_history::SwapService},
    },
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        profile::{
            CreatedTokensResponse, HoldTokenResponse, PointHistoryResponse, ProfileResponse,
            SwapHistoryResponse,
        },
    },
    utils::valid_evm_address,
};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use tracing::{error, instrument};

use super::path::ProfilePath;

/// Get account profile
#[utoipa::path(
    get,
    path = ProfilePath::GetProfile.docs_str(),
    params(
        ("account_id" = String, Path, description = "User's nickname or Ethereum address")
    ),
    responses(
        (status = 200, description = "User profile retrieved successfully", body = ProfileResponse),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_profile(
    Path(account_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<ProfileResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }

    let account_controller = AccountController::new(state.postgres.clone());
    let account_info = account_controller
        .get_account(&account_id)
        .await
        .map_err(|err| {
            error!(
                "Failed to get profile: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(ProfileResponse { account_info }))
}

/// Get profile positions with pagination
#[utoipa::path(
    get,
    path = ProfilePath::GetHoldToken.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get hold token for"),
        ("page" = i64, Query, description = "Page number (starts from 1)"),
        ("limit" = i64, Query, description = "Number of items per page"),
    ),
    responses(
        (status = 200, description = "Successfully retrieved positions", body = HoldTokenResponse),
        (status = 404, description = "Account invalid"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_hold_token(
    Path(account_id): Path<String>,
    Query(query): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<HoldTokenResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let position_service = PositionService::new(state.postgres.clone(), state.redis.clone());
    let response = position_service
        .get_hold_token_by_account(&account_id, &query)
        .await?;
    Ok(Json(response))
}

/// Get tokens created by profile with pagination
#[utoipa::path(
    get,
    path = ProfilePath::GetTokenCreated.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get created tokens for"),
        ("page" = i32, Query, description = "Page number (starts from 1)"),
        ("limit" = i32, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved created tokens", body = CreatedTokensResponse),
        (status = 404, description = "Account invalid"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_token_created(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<CreatedTokensResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let token_created_service =
        TokenCreatedService::new(state.postgres.clone(), state.redis.clone());
    let response = token_created_service
        .get_tokens_created(&account_id, &pagination)
        .await?;
    Ok(Json(response))
}

/// Get trade history with pagination
#[utoipa::path(
    get,
    path = ProfilePath::GetSwapHistory.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get trade history for"),
        ("page" = i32, Query, description = "Page number (starts from 1)"),
        ("limit" = i32, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved trade history", body = SwapHistoryResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_swap_history(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<SwapHistoryResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let swap_service = SwapService::new(state.postgres.clone(), state.redis.clone());
    let response = swap_service
        .get_swaps_by_account(&account_id, pagination)
        .await
        .map_err(|err| {
            error!(
                "Failed to get swap history: account_id: {}, error: {:?}",
                account_id, err
            );
            err
        })?;
    Ok(Json(response))
}

/// Get Point History
#[utoipa::path(
    get,
    path = ProfilePath::GetPointHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Point history fetched successfully", body = PointHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_point_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<PointHistoryResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_hype_point_history(&session_address, &params)
        .await?;

    Ok(Json(response))
}
