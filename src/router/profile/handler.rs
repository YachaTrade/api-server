use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        account::{AccountController, AccountResponse, RequestAccountIdParam},
        common::{identifier::Identifier, pagination::PaginationParams},
        token::create_token::{TokenCreatedController, TokenCreatedResponse},
        trading::{
            position::{HoldTokenResponse, PositionController, PositionResponse},
            swap_history::{PositionSwapResponse, SwapController},
        },
    },
    utils::valid_evm_address,
};

use axum::{
    extract::{Path, Query, State},
    Json,
};
use tracing::{error, info, instrument};

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

    let identifier: Identifier = account_id.clone().into();
    let account_controller = AccountController::new(state.postgres.clone());
    let account = account_controller
        .get_account_with_mutual(&identifier, params.request_account_id)
        .await
        .map_err(|err| {
            error!(
                "Failed to get profile: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Profile: account_id :{} response :{:?}",
        account_id, account
    );
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
    if let Ok(cached_response) = state
        .trade_redis
        .get_account_hold_token(&account_id, &query)
        .await
    {
        return Ok(Json(cached_response));
    }
    let pagination = PaginationParams {
        page: query.page,
        limit: query.limit,
        direction: "DESC".to_string(),
    };
    let position_controller = PositionController::new(state.postgres.clone());
    let response = position_controller
        .get_hold_token_by_account(&account_id, &pagination)
        .await
        .map_err(|err| {
            error!(
                "Failed to get position: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Hold Token: account_id :{} response :{:?}",
        account_id, response
    );
    if let Err(err) = state
        .trade_redis
        .set_account_hold_token(&account_id, &query, &response)
        .await
    {
        error!(
            "Failed to set hold token: account_id: {}, error: {}",
            account_id, err
        );
    }
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
    if let Ok(cached_response) = state
        .trade_redis
        .get_account_token_created(&account_id, &pagination)
        .await
    {
        return Ok(Json(cached_response));
    }
    let token_created_controller = TokenCreatedController::new(state.postgres.clone());
    let response = token_created_controller
        .get_tokens_created(&account_id, &pagination)
        .await
        .map_err(|err| {
            error!(
                "Failed to get token created: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Token Created: account_id :{} response :{:?}",
        account_id, response
    );
    if let Err(err) = state
        .trade_redis
        .set_account_token_created(&account_id, &pagination, &response)
        .await
    {
        error!(
            "Failed to set token created: account_id: {}, error: {}",
            account_id, err
        );
    }
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
    let swap_controller = SwapController::new(state.postgres.clone());
    let response = swap_controller
        .get_swaps_by_account(&account_id, pagination)
        .await
        .map_err(|err| {
            error!(
                "Failed to get swap history: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Swap History: account_id :{} response :{:?}",
        account_id, response
    );
    Ok(Json(response))
}
