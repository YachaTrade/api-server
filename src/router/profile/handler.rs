use crate::{
    result::AppJsonResult,
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
        ("target" = String, Path, description = "User's nickname or Ethereum address"),
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
    Query(params): Query<AccountParams>,
    State(state): State<AppState>,
) -> AppJsonResult<AccountResponse> {
    let AccountParams {
        target,
        request_account_id,
    } = params;

    let identifier: Identifier = target.into();
    let account = AccountController::new(state.postgres.clone())
        .get_account_with_mutual(&identifier, request_account_id)
        .await?;
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
        (status = 404, description = "Account not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_pnl(
    Path(account_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<PNLResponse> {
    let pnl = PNLController::new(state.postgres.clone())
        .get_pnl(&account_id)
        .await?;
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
        (status = 404, description = "Account not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_position(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<PositionResponse> {
    let position_controller = PositionController::new(state.postgres.clone());
    let positions = position_controller
        .get_positions(&account_id, pagination)
        .await?;
    Ok(Json(PositionResponse { positions }))
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
        (status = 404, description = "Account not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_token_created(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenCreatedResponse> {
    let tokens = TokenCreatedController::new(state.postgres.clone())
        .get_tokens_created(&account_id, pagination)
        .await?;
    Ok(Json(TokenCreatedResponse { tokens }))
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
    let swaps = SwapController::new(state.postgres.clone())
        .get_swaps(&account_id, pagination)
        .await?;
    Ok(Json(SwapResponse { swaps }))
}
