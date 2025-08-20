use axum::{Extension, Json, extract::State};

use tracing::{instrument, warn};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::account::{
        AccountController, AccountResponse, UpdateAccountRequest,
        wallet::{AccountWalletResponse, RegisterWalletRequest, WalletController},
        x::{
            AccountXController, ConnectXRequest, ConnectedXAccountResponse, DisconnectXRequest,
            DisconnectedXAccountResponse, GetXHandleResponse,
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
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateAccountRequest>,
) -> AppJsonResult<AccountResponse> {
    let UpdateAccountRequest {
        nickname,
        bio,
        image_uri,
    } = payload;
    // 이미지나 텍스트 필드 중 하나는 업데이트되어야 함
    if nickname.is_none() && bio.is_none() && image_uri.is_none() {
        warn!("Update account Error: At least one of nickName or bio, or image must be provided");
        return Err(AppError::BadRequest(
            "At least one of nickName, bio, or image must be provided".into(),
        ));
    }
    //checking url
    if let Some(image_uri) = &image_uri {
        if !image_uri.starts_with("https://storage.nadapp.net/profile/") {
            warn!("Update account Error: Invalid image URL");
            return Err(AppError::BadRequest("Invalid image URL".into()));
        }
    }

    // @ = x handle 전용
    if let Some(nickname) = &nickname {
        if nickname.starts_with('@') || nickname.starts_with('#') {
            warn!("Update account Error: Nickname cannot start with @ or #");
            return Err(AppError::BadRequest(
                "Nickname cannot start with @ or #".into(),
            ));
        }
    }

    // 텍스트 필드 처리
    let clean_text =
        |text: Option<String>| text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());

    // 계정 업데이트
    let account_controller = AccountController::new(state.postgres.clone());
    let updated_account = account_controller
        .update_account(
            &session_address,
            clean_text(image_uri),
            clean_text(nickname),
            clean_text(bio),
        )
        .await
        .map_err(|e| {
            warn!("Update account Error {:?}", e);
            AppError::InternalError(e.to_string())
        })?;

    Ok(Json(AccountResponse {
        account: updated_account,
    }))
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
    let account_controller = AccountController::new(state.postgres.clone());
    let account = account_controller
        .get_account(&session_address)
        .await
        .map_err(|err| {
            warn!("get account Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(AccountResponse { account }))
}

// Account Connect X
#[utoipa::path(
    put,
    path = AccountPath::ConnectX.docs_str(),
    request_body = ConnectXRequest,
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
pub async fn connect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ConnectXRequest>,
) -> AppJsonResult<ConnectedXAccountResponse> {
    payload
        .validate()
        .map_err(|err| AppError::BadRequest(err.to_string()))?;
    let account_x_controller = AccountXController::new(state.postgres.clone());
    let response = account_x_controller
        .connect_x(&session_address, payload)
        .await
        .map_err(|err| {
            warn!("connect x account Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;
    Ok(Json(response))
}

// Account Disconnect X
#[utoipa::path(
    delete,
    path = AccountPath::DisconnectX.docs_str(),
    request_body = DisconnectXRequest,
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
pub async fn disconnect_x(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<DisconnectXRequest>,
) -> AppJsonResult<DisconnectedXAccountResponse> {
    let DisconnectXRequest { x_handle } = payload;
    let account_x_controller = AccountXController::new(state.postgres.clone());
    let response = account_x_controller
        .disconnect_x(session_address, x_handle)
        .await
        .map_err(|err| {
            warn!("disconnect x account Error {:?}", err);
            // X 핸들을 찾을 수 없는 경우 NotFound 오류 반환
            if err.to_string().contains("not found") {
                AppError::NotFound(err.to_string())
            } else {
                AppError::BadRequest(err.to_string())
            }
        })?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = AccountPath::GetX.docs_str(),
    responses(
        (status = 200, description = "Get x handle successfully", body = GetXHandleResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]

pub async fn get_x_handle(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<GetXHandleResponse> {
    let account_x_controller = AccountXController::new(state.postgres.clone());
    let response = account_x_controller
        .get_x_handle(session_address)
        .await
        .map_err(|err| {
            warn!("get x handle Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;
    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = AccountPath::RegisterWallet.docs_str(),
    request_body = RegisterWalletRequest,
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
pub async fn register_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<RegisterWalletRequest>,
) -> AppJsonResult<AccountWalletResponse> {
    let response = WalletController::new(state.postgres.clone())
        .register_wallet(session_address, payload.wallet)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = AccountPath::GetWallet.docs_str(),
    responses(
        (status = 200, description = "Get account successfully", body = AccountWalletResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]
pub async fn get_wallet(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountWalletResponse> {
    let response = WalletController::new(state.postgres.clone())
        .get_wallet(session_address)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(response))
}
