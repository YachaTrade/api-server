use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        account::{AccountController, AccountResponse, RequestAccountIdParam},
        common::{identifier::Identifier, pagination::PaginationParams},
        token::create_token::{TokenCreatedController, TokenCreatedResponse},
        trading::{
            pnl::{PNLController, PNLResponse},
            position::{PositionController, PositionQuery, PositionResponse, PositionType},
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
#[instrument(skip_all)]
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

/// Get profile PNL information
#[utoipa::path(
    get,
    path = ProfilePath::GetPnl.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get PNL information for")
    ),
    responses(
        (status = 200, description = "Successfully retrieved PNL information", body = PNLResponse),
        (status = 404, description = "Account invalid"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip_all)]
pub async fn get_pnl(
    Path(account_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<PNLResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let pnl_controller = PNLController::new(state.postgres.clone());
    let pnl = pnl_controller.get_pnl(&account_id).await.map_err(|err| {
        error!(
            "Failed to get PNL: account_id: {}, error: {}",
            account_id, err
        );
        AppError::InternalError(err.to_string())
    })?;
    info!("Get PNL: account_id :{} pnl :{:?}", account_id, pnl);
    Ok(Json(pnl))
}

/// Get profile positions with pagination
#[utoipa::path(
    get,
    path = ProfilePath::GetPosition.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get positions for"),
        ("page" = i64, Query, description = "Page number (starts from 1)"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("position_type" = String, Query, description = "Type of position to get (ALL, OPEN, CLOSE)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved positions", body = PositionResponse),
        (status = 404, description = "Account invalid"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
#[instrument(skip_all)]
pub async fn get_position(
    Path(account_id): Path<String>,
    Query(query): Query<PositionQuery>,
    State(state): State<AppState>,
) -> AppJsonResult<PositionResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let pagination = PaginationParams {
        page: query.page,
        limit: query.limit,
    };
    let position_controller = PositionController::new(state.postgres.clone());
    let response = position_controller
        .get_positions(&account_id, pagination, query.position_type)
        .await
        .map_err(|err| {
            error!(
                "Failed to get position: account_id: {}, error: {}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Position: account_id :{} response :{:?}",
        account_id, response
    );
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
#[instrument(skip_all)]
pub async fn get_token_created(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenCreatedResponse> {
    if !valid_evm_address(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let token_created_controller = TokenCreatedController::new(state.postgres.clone());
    let response = token_created_controller
        .get_tokens_created(&account_id, pagination)
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
#[instrument(skip_all)]
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
