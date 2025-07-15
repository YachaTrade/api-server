use axum::{
    extract::{Query, State},
    Extension, Json,
};

use tracing::{error, instrument, warn};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        management::{
            DevPositionsResponse, HoldingTokenManagementResponse, TokenLockResponse,
            TokenManagementController, WithdrawableLockResponse,
        },
    },
};

use super::path::ManagementPath;

/// Get developer positions
#[utoipa::path(
    get,
    path = ManagementPath::DevPosition.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC")
    ),
    responses(
        (status = 200, description = "Dev positions retrieved successfully", body = DevPositionsResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Token Management"
)]
#[instrument(skip(state))]
pub async fn get_dev_positions(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<DevPositionsResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_dev_positions(&session_address, &query)
        .await
    {
        return Ok(Json(cached_response));
    }
    let management_controller = TokenManagementController::new(state.postgres.clone());
    let dev_position = management_controller
        .get_dev_positions(&session_address, &query)
        .await
        .map_err(|err| {
            error!("Failed to get dev positions: {}", err);
            AppError::InternalError(err.to_string())
        })?;

    if let Err(err) = state
        .redis
        .set_dev_positions(&session_address, &query, &dev_position)
        .await
    {
        warn!(
            "Failed to set dev positions account_id: {}, pagination: {:?}, error: {}",
            session_address, query, err
        );
    }

    Ok(Json(dev_position))
}

/// Get Holding Token Management
#[utoipa::path(
    get,
    path = ManagementPath::HoldingTokenManagements.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC")
    ),
    responses(
        (status = 200, description = "Management positions retrieved successfully", body = HoldingTokenManagementResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Token Management"
)]
pub async fn get_holding_token_management(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<HoldingTokenManagementResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_holding_token_management(&session_address, &query)
        .await
    {
        return Ok(Json(cached_response));
    }
    let management_controller = TokenManagementController::new(state.postgres.clone());
    let management_position = management_controller
        .get_holding_token_management(&session_address, &query)
        .await
        .map_err(|err| {
            error!("Failed to get holding token management: {}", err);
            AppError::InternalError(err.to_string())
        })?;

    if let Err(err) = state
        .redis
        .set_holding_token_management(&session_address, &query, &management_position)
        .await
    {
        warn!(
            "Failed to set holding token management account_id: {}, pagination: {:?}, error: {}",
            session_address, query, err
        );
    }

    Ok(Json(management_position))
}

/// Get Account Locks
#[utoipa::path(
    get,
    path = ManagementPath::AccountLocks.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC")
    ),
    responses(
        (status = 200, description = "Account locks retrieved successfully", body = TokenLockResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Token Management"
)]
pub async fn get_account_locks(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<TokenLockResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_account_locks(&session_address, &query)
        .await
    {
        return Ok(Json(cached_response));
    }
    let management_controller = TokenManagementController::new(state.postgres.clone());
    let account_locks = management_controller
        .get_account_locks(&session_address, &query)
        .await
        .map_err(|err| {
            error!("Failed to get account locks: {}", err);
            AppError::InternalError(err.to_string())
        })?;

    if let Err(err) = state
        .redis
        .set_account_locks(&session_address, &query, &account_locks)
        .await
    {
        warn!(
            "Failed to set account locks account_id: {}, pagination: {:?}, error: {}",
            session_address, query, err
        );
    }

    Ok(Json(account_locks))
}

/// Get Account Withdrawable Lock
#[utoipa::path(
    get,
    path = ManagementPath::AccountWithdrawableLock.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC")
    ),
    responses(
        (status = 200, description = "Account withdrawable lock retrieved successfully", body = WithdrawableLockResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Token Management"
)]
pub async fn get_account_withdrawable_lock(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<WithdrawableLockResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_account_withdrawable_lock(&session_address, &query)
        .await
    {
        return Ok(Json(cached_response));
    }
    let management_controller = TokenManagementController::new(state.postgres.clone());
    let account_withdrawable_lock = management_controller
        .get_account_withdrawable_lock(&session_address, &query)
        .await
        .map_err(|err| {
            error!("Failed to get account withdrawable lock: {}", err);
            AppError::InternalError(err.to_string())
        })?;

    if let Err(err) = state
        .redis
        .set_account_withdrawable_lock(&session_address, &query, &account_withdrawable_lock)
        .await
    {
        warn!(
            "Failed to set account withdrawable lock account_id: {}, pagination: {:?}, error: {}",
            session_address, query, err
        );
    }

    Ok(Json(account_withdrawable_lock))
}
