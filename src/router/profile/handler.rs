use crate::{
    controllers::account::AccountController,
    result::{AppError, AppJsonResult},
    services::{
        token::create::TokenCreatedService,
        trading::{position::PositionService, swap_history::SwapService},
    },
    state::AppState,
    types::{
        account::{AccountResponse, RequestAccountIdParam},
        common::pagination::PaginationParams,
        token::create_token::TokenCreatedResponse,
        trading::{position::HoldTokenResponse, swap_history::PositionSwapResponse},
    },
    utils::valid_evm_address,
};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use tracing::{error, instrument};

use super::path::ProfilePath;

/// Get account profile
#[utoipa::path(
    get,
    path = ProfilePath::GetProfile.docs_str(),
    params(
        ("account_id" = String, Path, description = "User's nickname or Ethereum address"),
        ("request_account_id" = Option<String>, Query, description = "Viewer's account ID to calculate mutual friends")
    ),
    responses(
        (status = 200, description = "User profile retrieved successfully", body = AccountResponse),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip(state))]
pub async fn get_profile(
    Path(account_id): Path<String>,
    Query(params): Query<RequestAccountIdParam>,
    State(state): State<AppState>,
) -> AppJsonResult<AccountResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }

    let account_controller = AccountController::new(state.postgres.clone());
    let account = account_controller
        .get_account(&account_id)
        .await
        .map_err(|err| {
            error!(
                "Failed to get profile: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(AccountResponse { account }))
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
        (status = 200, description = "Successfully retrieved created tokens", body = TokenCreatedResponse),
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
) -> AppJsonResult<TokenCreatedResponse> {
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
        (status = 200, description = "Successfully retrieved trade history", body = PositionSwapResponse),
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
) -> AppJsonResult<PositionSwapResponse> {
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
