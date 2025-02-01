use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        account::{AccountController, AccountParams, AccountResponse},
        common::{identifier::Identifier, pagination::PaginationParams},
        token::create_token::{TokenCreatedController, TokenCreatedResponse},
        trading::{
            pnl::{PNLController, PNLResponse},
            position::{PositionController, PositionResponse},
            swap_history::{SwapController, SwapResponse},
        },
    },
};

use axum::{
    extract::{Path, Query, State},
    Json,
};

use super::path::ProfilePath;

/// Get user profile
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

pub async fn get_profile(
    Path(account_id): Path<String>,
    Query(request_account_id): Query<Option<String>>,
    State(state): State<AppState>,
) -> AppJsonResult<AccountResponse> {
    if !valid_account_id(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }

    let identifier: Identifier = account_id.into();
    let account = AccountController::new(state.postgres.clone())
        .get_account_with_mutual(&identifier, request_account_id)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
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
pub async fn get_pnl(
    Path(account_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<PNLResponse> {
    if !valid_account_id(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let pnl = PNLController::new(state.postgres.clone())
        .get_pnl(&account_id)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(Json(pnl))
}

/// Get profile positions with pagination
#[utoipa::path(
    get,
    path = ProfilePath::GetPosition.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get positions for"),
        ("page" = i32, Query, description = "Page number (starts from 1)"),
        ("limit" = i32, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved positions", body = PositionResponse),
        (status = 404, description = "Account invalid"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_position(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<PositionResponse> {
    if !valid_account_id(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let position_controller = PositionController::new(state.postgres.clone());
    let response = position_controller
        .get_positions(&account_id, pagination)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
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

pub async fn get_token_created(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenCreatedResponse> {
    if !valid_account_id(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let response = TokenCreatedController::new(state.postgres.clone())
        .get_tokens_created(&account_id, pagination)
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
        (status = 200, description = "Successfully retrieved trade history", body = SwapResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_swap_history(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<SwapResponse> {
    if !valid_account_id(&account_id) {
        return Err(AppError::BadRequest("Invalid account ID".to_string()));
    }
    let swap_controller = SwapController::new(state.postgres.clone());
    let response = swap_controller
        .get_swaps(&account_id, pagination)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(Json(response))
}

fn valid_account_id(account_id: &str) -> bool {
    account_id.starts_with("0x")
        && account_id.len() == 42
        && account_id[2..].chars().all(|c| c.is_ascii_hexdigit())
}
