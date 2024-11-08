use std::str::FromStr;

use alloy::{primitives::Address, signers::Signature};
use axum::{
    extract::State,
    http::{HeaderValue, Response, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use base64::{prelude::BASE64_STANDARD, Engine};

use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};

use tracing::{info, instrument};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    constant::EXPIRATION_SESSION_KEY,
    db::postgres::{
        controller::{account::AccountController, session::SessionController},
        model::Account,
    },
    env,
    result::{AppError, AppJsonResult, AppResult},
    state::AppState,
};

use super::path::Path;

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "address": "Your address"
}))]
pub struct AuthNonceRequest {
    #[schema(example = "Your address")]
    address: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "nonce": "abced-abced-abced"
}))]
pub struct AuthNonceResponse {
    #[schema(example = "abced-abced-abced")]
    nonce: String,
}
/// Generate authentication nonce
#[utoipa::path(
    post,
    path = Path::Nonce.as_str(),
    operation_id = "Generate authentication nonce", 
    request_body = AuthNonceRequest,
    responses(
        (status = 200, description = "Nonce generated successfully", body = AuthNonceResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Auth"
)]
#[instrument(skip(state))]
pub async fn auth_nonce(
    State(state): State<AppState>,
    Json(payload): Json<AuthNonceRequest>,
) -> AppJsonResult<AuthNonceResponse> {
    let nonce = Uuid::new_v4().to_string();
    Address::from_str(&payload.address)
        .map_err(|e| AppError::BadRequest("Invalid address".to_string()))?;
    let redis = state.redis.clone();
    redis
        .set_nonce(&payload.address, &nonce)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;

    Ok(Json(AuthNonceResponse { nonce }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "signature": "Signature with nonce signed with private key",
    "nonce": "Get nonce from /auth/nonce"
}))]
pub struct AuthSessionRequest {
    #[schema(example = "0x1234567890abcdef...")]
    signature: String,
    #[schema(example = "abcdef-abcedef-abcedf")]
    nonce: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    account: Account,
}
/// Generate authentication session
#[utoipa::path(
    post,
    path = Path::Session.as_str(),
    operation_id = "Generate authentication session", 
    request_body = AuthSessionRequest,
    responses(
        (status = 200, description = "Session created successfully", body = AuthSessionResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Auth"
)]
#[instrument(skip(state, payload))]
pub async fn auth_session(
    State(state): State<AppState>,
    Json(payload): Json<AuthSessionRequest>,
) -> AppResult<impl IntoResponse> {
    // 1. 주소와 서명 파싱
    let nonce = payload.nonce;
    let signature = Signature::from_str(&payload.signature)
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    // 2. 서명에서 주소 복구
    let address = signature
        .recover_address_from_msg(nonce.clone())
        .map_err(|_| AppError::BadRequest("Invalid signature".to_string()))?
        .to_string();
    info!("Recovered address = {:?}", address);
    let redis = state.redis.clone();

    let session_nonce = redis
        .get_nonce(&address)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;
    info!("Session_nonce = {:?}", session_nonce);
    if nonce != session_nonce {
        AppError::Unauthorized("Invalid nonce".to_string());
    }
    redis
        .del_nonce(&address)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;
    let session_id = generate_session_id(address.as_str());
    redis
        .set_session(&session_id, &address, *EXPIRATION_SESSION_KEY)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;
    //session key는 postgres 에 어떻게 저장할거냐?
    let postgres = state.postgres.clone();

    // 여기선 account 가 없을수가 없음
    let account_controller = AccountController::new(postgres.clone());
    let account = account_controller
        .get_or_create_account(&address)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let session_controller = SessionController::new(postgres.clone());

    session_controller
        .set_session(&session_id, &account.account_id)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let max_age = 7 * 24 * 60 * 60; // 7일

    let cookie = {
        let environment = env::get_env("ENVIRONMENT");

        let ip = env::get_env("IP");
        if environment == "development" {
            format!(
                "session={}; Path=/; Max-Age={}; Domain={}",
                session_id, max_age, ip
            )
        } else {
            format!(
                "session={}; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age={}; Domain={}",
                session_id, max_age, ip
            )
        }
    };

    let body = Json(AuthSessionResponse { account });
    let response = Response::builder()
        .header(
            axum::http::header::SET_COOKIE,
            HeaderValue::from_str(&cookie).unwrap(),
        )
        .body(body.into_response())
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(response)
}
/// Delete authentication session
#[utoipa::path(
    delete,
    path = Path::DeleteSession.as_str(),
    operation_id = "Delete authentication session", 
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Session deleted successfully"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Auth"
)]
#[instrument(skip(state))]
pub async fn auth_delete_session(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppResult<impl IntoResponse> {
    SessionController::new(state.postgres.clone())
        .delete_session_by_id(&session_address)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let redis = state.redis.clone();
    redis
        .delete_session(&session_address)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;

    Ok(StatusCode::OK.into_response())
}

//충돌 방지
//session 키를 주소와
fn generate_session_id(address: &str) -> String {
    // UUID 생성
    let uuid = Uuid::new_v4();

    // 나노초 단위의 타임스탬프
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // 랜덤 논스 생성
    let nonce: u64 = thread_rng().gen();

    // 블록체인 주소, 체인ID, 타임스탬프, UUID, 논스를 모두 결합
    let combined = format!(
        "{}-{}-{}-{}",
        address,   // 유저 address
        timestamp, // 타임스탬프
        uuid,      // UUID
        nonce      // 랜덤 논스
    );

    // Base64로 인코딩
    BASE64_STANDARD.encode(combined.as_bytes())[..32].to_string()
}
