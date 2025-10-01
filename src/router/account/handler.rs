use axum::{Extension, Json, extract::State};

use tracing::{info, instrument};

use crate::{
    result::AppJsonResult,
    services::account::AccountService,
    state::AppState,
    types::account::{
        AccountResponse, UpdateAccountRequest,
        wallet::{AccountWalletResponse, RegisterWalletRequest},
        x::{
            ConnectXRequest, ConnectedXAccountResponse, DisconnectXRequest,
            DisconnectedXAccountResponse, GetXHandleResponse, UpdateXRequest,
        },
    },
};

use super::path::AccountPath;

/// Update account profile
#[utoipa::path(
    patch,
    path = AccountPath::UpdateAccount.docs_str(),
    request_body = UpdateAccountRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Account updated successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),

    tag="Account"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateAccountRequest>,
) -> AppJsonResult<AccountResponse> {
    info!("update account: {}", payload);
    let service = AccountService::new(state.postgres.clone());
    let response = service.update_account(&session_address, payload).await?;

    Ok(Json(response))
}

/// Get account session check
#[utoipa::path(
    get,
    path = AccountPath::GetAccount.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address))]
pub async fn get_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.get_account(&session_address).await?;

    Ok(Json(response))
}

// Account Connect X
#[utoipa::path(
    put,
    path = AccountPath::ConnectX.docs_str(),
    request_body = ConnectXRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),

    tag="Account"
)]
pub async fn connect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ConnectXRequest>,
) -> AppJsonResult<ConnectedXAccountResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.connect_x(&session_address, payload).await?;
    Ok(Json(response))
}

// Account Disconnect X
#[utoipa::path(
    delete,
    path = AccountPath::DisconnectX.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),

    tag="Account"
)]
pub async fn disconnect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<DisconnectedXAccountResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.disconnect_x(session_address).await?;
    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = AccountPath::UpdateX.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = UpdateXRequest,
    responses(
        (status = 200, description = "Update x successfully", body = GetXHandleResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]

pub async fn update_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateXRequest>,
) -> AppJsonResult<GetXHandleResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.update_x(session_address, payload).await?;
    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = AccountPath::RegisterWallet.docs_str(),
    request_body = RegisterWalletRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),

    tag="Account"
)]
pub async fn register_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<RegisterWalletRequest>,
) -> AppJsonResult<AccountWalletResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.register_wallet(session_address, payload).await?;

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = AccountPath::GetWallet.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountWalletResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),

    tag="Account"
)]
pub async fn get_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountWalletResponse> {
    let service = AccountService::new(state.postgres.clone());
    let response = service.get_wallet(session_address).await?;

    Ok(Json(response))
}
