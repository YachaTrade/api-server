use axum::{Extension, Json, extract::State};

use tracing::{error, info, instrument};

use crate::{
    result::{AppError, AppJsonResult},
    services::account::AccountService,
    state::AppState,
    types::account::{
        AccountResponse, ConnectXRequest, GetWalletResponse, RegisterWalletRequest,
        UpdateAccountRequest, UpdateXRequest,
    },
};

use super::path::AccountPath;

/// Update account profile
#[utoipa::path(
    patch,
    path = AccountPath::UpdateAccount.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = UpdateAccountRequest,
    responses(
        (status = 200, description = "Account updated successfully", body = AccountResponse)
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateAccountRequest>,
) -> AppJsonResult<AccountResponse> {
    info!("update account: {:?}", payload);

    payload.validate().map_err(|e| {
        error!("Invalid update account request: {}", e);
        AppError::BadRequest(e)
    })?;

    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.update_account(&session_address, payload).await?;

    Ok(Json(response))
}

/// Get account
#[utoipa::path(
    get,
    path = AccountPath::GetAccount.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse)
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address))]
pub async fn get_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountResponse> {
    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_account(&session_address).await?;

    Ok(Json(response))
}

/// Connect X account
#[utoipa::path(
    put,
    path = AccountPath::ConnectX.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = ConnectXRequest,
    responses(
        (status = 200, description = "X account connected successfully", body = AccountResponse)
    ),
    tag="Account"
)]
pub async fn connect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ConnectXRequest>,
) -> AppJsonResult<AccountResponse> {
    payload.validate().map_err(|e| {
        error!("Invalid connect_x request: {}", e);
        AppError::BadRequest(e)
    })?;

    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.connect_x(&session_address, payload).await?;
    Ok(Json(response))
}

/// Disconnect X account
#[utoipa::path(
    delete,
    path = AccountPath::DisconnectX.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "X account disconnected successfully", body = AccountResponse)
    ),
    tag="Account"
)]
pub async fn disconnect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountResponse> {
    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.disconnect_x(session_address).await?;
    Ok(Json(response))
}

/// Update X account
#[utoipa::path(
    patch,
    path = AccountPath::UpdateX.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = UpdateXRequest,
    responses(
        (status = 200, description = "X account updated successfully", body = AccountResponse)
    ),
    tag="Account"
)]
pub async fn update_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateXRequest>,
) -> AppJsonResult<AccountResponse> {
    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.update_x(session_address, payload).await?;
    Ok(Json(response))
}

/// Register wallet
#[utoipa::path(
    patch,
    path = AccountPath::RegisterWallet.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = RegisterWalletRequest,
    responses(
        (status = 200, description = "Wallet registered successfully", body = AccountResponse)
    ),
    tag="Account"
)]
pub async fn register_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<RegisterWalletRequest>,
) -> AppJsonResult<AccountResponse> {
    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.register_wallet(session_address, payload).await?;

    Ok(Json(response))
}

/// Get wallet
#[utoipa::path(
    get,
    path = AccountPath::GetWallet.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Wallet retrieved successfully", body = GetWalletResponse)
    ),
    tag="Account"
)]
pub async fn get_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<GetWalletResponse> {
    let service = AccountService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_wallet(session_address).await?;

    Ok(Json(response))
}
