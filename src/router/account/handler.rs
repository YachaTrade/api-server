use axum::{Extension, Json, extract::State, http::HeaderMap};
use bytes::Bytes;

use tracing::{error, info, instrument};

use crate::{
    result::{AppError, AppJsonResult},
    services::account::AccountService,
    state::AppState,
    types::account::{
        AccountResponse, ConnectXRequest, GetWalletResponse, RegisterWalletRequest,
        UpdateAccountRequest, UpdateXRequest, UploadImageResponse,
    },
};

use super::path::AccountPath;

const ALLOWED_IMAGE_TYPES: [&str; 6] = [
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/heic",
];
const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5MB

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
    payload.validate().map_err(|e| {
        error!("Invalid update_x request: {}", e);
        AppError::BadRequest(e)
    })?;

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

/// Upload profile image
#[utoipa::path(
    post,
    path = AccountPath::UploadImage.docs_str(),
    request_body(
        content = Vec<u8>,
        description = "Raw image binary data (supported formats: image/jpeg, image/jpg, image/png, image/gif, image/webp, image/heic)",
        content_type = "image/png"
    ),
    responses(
        (status = 200, description = "Image uploaded successfully", body = UploadImageResponse),
        (status = 400, description = "Bad request - Invalid image format or missing image"),
        (status = 413, description = "Payload too large - Image exceeds 5MB limit"),
        (status = 500, description = "Internal server error - Upload failed")
    ),
    tag = "Account"
)]
pub async fn upload_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppJsonResult<UploadImageResponse> {
    info!("Starting profile image upload - Size: {} bytes", body.len());

    // Validate image size
    if body.len() > MAX_IMAGE_SIZE {
        error!("Image too large: {} bytes", body.len());
        return Err(AppError::BadRequest(format!(
            "Image size exceeds maximum allowed size of {} MB",
            MAX_IMAGE_SIZE / 1024 / 1024
        )));
    }

    // Validate content type
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            error!("Missing content-type header");
            AppError::BadRequest("Missing content-type header".to_string())
        })?;

    if !ALLOWED_IMAGE_TYPES.contains(&content_type) {
        error!("Invalid content type: {}", content_type);
        return Err(AppError::BadRequest(format!(
            "Invalid image type. Allowed types: {:?}",
            ALLOWED_IMAGE_TYPES
        )));
    }

    // Generate UUID for image
    let image_id = uuid::Uuid::new_v4().to_string();

    // Upload to R2
    let image_uri = state
        .r2
        .upload_account_image_file(&image_id, &body, content_type)
        .await
        .map_err(|e| {
            error!("Failed to upload image to R2: {}", e);
            AppError::InternalError(format!("Failed to upload image: {}", e))
        })?;

    info!("Profile image uploaded successfully: {}", image_uri);

    Ok(Json(UploadImageResponse { image_uri }))
}
