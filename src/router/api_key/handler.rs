use axum::{
    Extension, Json,
    extract::{Path, State},
};
use tracing::instrument;
use uuid::Uuid;

use super::path::ApiKeyPath;
use crate::{
    result::AppJsonResult,
    services::api_key::{create_api_key, list_api_keys_by_owner, revoke_api_key_by_owner},
    state::AppState,
    types::api_key::{ApiKeyInfo, ApiKeyListResponse, CreateApiKeyRequest, CreateApiKeyResponse},
};

/// Create a new API key for the authenticated user
#[utoipa::path(
    post,
    path = ApiKeyPath::ApiKey.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created successfully", body = CreateApiKeyResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Key"
)]
#[instrument(skip(state, payload))]
pub async fn create_api_key_handler(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(mut payload): Json<CreateApiKeyRequest>,
) -> AppJsonResult<CreateApiKeyResponse> {
    // owner_address를 세션 주소로 자동 설정
    payload.owner_address = Some(session_address);
    let response = create_api_key(&state.postgres, payload).await?;
    Ok(Json(response))
}

/// List API keys for the authenticated user
#[utoipa::path(
    get,
    path = ApiKeyPath::ApiKey.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "API keys retrieved successfully", body = ApiKeyListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Key"
)]
#[instrument(skip(state))]
pub async fn list_api_keys_handler(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<ApiKeyListResponse> {
    let keys = list_api_keys_by_owner(&state.postgres, &session_address).await?;
    let total = keys.len() as i64;
    let api_keys: Vec<ApiKeyInfo> = keys.into_iter().map(ApiKeyInfo::from).collect();

    Ok(Json(ApiKeyListResponse { api_keys, total }))
}

/// Revoke an API key (only if owned by the authenticated user)
#[utoipa::path(
    delete,
    path = ApiKeyPath::ApiKeyId.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("id" = Uuid, Path, description = "API key ID to revoke")
    ),
    responses(
        (status = 200, description = "API key revoked successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "API key not found or not owned by user"),
        (status = 500, description = "Internal server error")
    ),
    tag = "API Key"
)]
#[instrument(skip(state))]
pub async fn revoke_api_key_handler(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(id): Path<Uuid>,
) -> AppJsonResult<serde_json::Value> {
    revoke_api_key_by_owner(&state.postgres, &state.redis, id, &session_address).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}
