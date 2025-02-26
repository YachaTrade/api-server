use std::env;
use std::str::FromStr;

use alloy::{primitives::Address, signers::Signature};
use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::{
    extract::State,
    http::{Response, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use base64::{prelude::BASE64_STANDARD, Engine};
use tower_cookies::cookie::time::Duration;
use tower_cookies::Cookie;

use tracing::{error, info, instrument};

use uuid::Uuid;

use crate::types::account::{Account, AccountController};
use crate::types::auth::{
    AuthNonceRequest, AuthNonceResponse, AuthSessionRequest, AuthSessionResponse, SessionController,
};
use crate::{
    config::EXPIRATION_SESSION_KEY,
    result::{AppError, AppJsonResult, AppResult},
    state::AppState,
};

use super::path::Path;

/// Generate authentication nonce
#[utoipa::path(
    post,
    path = Path::Nonce.as_str(),
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
        .map_err(|_e| AppError::BadRequest("Invalid address".to_string()))?;
    let redis = state.redis.clone();
    redis
        .set_nonce(&payload.address, &nonce)
        .await
        .map_err(|err| AppError::RedisError(err.to_string()))?;

    Ok(Json(AuthNonceResponse { nonce }))
}

/// Generate authentication session
#[utoipa::path(
    post,
    path = Path::Session.as_str(),
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
    let chain_id = payload.chain_id;
    let env_chain_id = env::var("CHAIN_ID")
        .expect("CHAIN_ID must be set")
        .parse::<u64>()
        .unwrap();

    if chain_id != env_chain_id {
        return Err(AppError::BadRequest("Invalid chain id".to_string()).into());
    }

    // 1. 주소와 서명 파싱
    let nonce = payload.nonce;
    let signature = Signature::from_str(&payload.signature)
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    // 2. 서명에서 주소 복구
    let address = signature
        .recover_address_from_msg(nonce.clone())
        .map_err(|_| AppError::BadRequest("Invalid signature".to_string()))?
        .to_string();

    let redis = state.redis.clone();
    info!("Nonce for address {}: {}", address, nonce);
    let session_nonce = redis.get_nonce(&address).await.map_err(|err| {
        error!("Failed to get nonce: address: {}, error: {}", address, err);
        AppError::RedisError(err.to_string())
    })?;
    info!("Session nonce for address {}: {}", address, session_nonce);
    if nonce != session_nonce {
        error!("Invalid nonce: address: {}, nonce: {}", address, nonce);
        return Err(AppError::Unauthorized("Invalid nonce".to_string()).into());
    }

    redis.del_nonce(&address).await.map_err(|err| {
        error!(
            "Failed to delete nonce: address: {}, error: {}",
            address, err
        );
        AppError::RedisError(err.to_string())
    })?;
    let session_id = generate_session_id(address.as_str(), nonce.as_str());

    redis
        .set_session(&session_id, &address, *EXPIRATION_SESSION_KEY)
        .await
        .map_err(|err| {
            error!(
                "Failed to set session: session_id: {}, address: {}, error: {}",
                session_id, address, err
            );
            AppError::RedisError(err.to_string())
        })?;
    //session key는 postgres에 어떻게 저장할거냐?

    let postgres = state.postgres.clone();

    // 여기선 account 가 없을수가 없음
    let account_controller = AccountController::new(postgres.clone());
    let account = Account::new(address.clone());
    let account = account_controller
        .upsert_account(account)
        .await
        .map_err(|err| {
            error!(
                "Failed to upsert account: address: {}, error: {}",
                address, err
            );
            AppError::InternalError(err.to_string())
        })?;

    let session_controller = SessionController::new(postgres.clone());

    session_controller
        .set_session(&session_id, &account.account_id)
        .await
        .map_err(|err| {
            error!(
                "Failed to set session: session_id: {}, account_id: {}, error: {}",
                session_id, account.account_id, err
            );
            AppError::InternalError(err.to_string())
        })?;

    //추후 프론트 배포시 samesite = strict 로 변경
    let mut cookie = Cookie::new("api-session", session_id);
    cookie.set_http_only(true);
    // // cookie.set_domain("nad.fun"); // 변경
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie.set_same_site(tower_cookies::cookie::SameSite::None);
    cookie.set_max_age(Duration::days(7));

    let body = Json(AuthSessionResponse { account });
    let response = Response::builder()
        .header(
            SET_COOKIE,
            HeaderValue::from_str(&cookie.to_string()).unwrap(),
        )
        .body(body.into_response())
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(response)
}

/// Delete authentication session
#[utoipa::path(
    delete,
    path = Path::DeleteSession.as_str(),
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

    // Remove session cookie by setting its expiry to a past date
    let mut cookie = Cookie::new("api-session", "");
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie.set_same_site(tower_cookies::cookie::SameSite::None);
    cookie.set_max_age(Duration::ZERO);

    let mut response = StatusCode::OK.into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie.to_string())
            .map_err(|err| AppError::InternalError(err.to_string()))?,
    );

    Ok(response)
}

//충돌 방지
//session 키를 주소와 nonce 로 생성
fn generate_session_id(address: &str, nonce: &str) -> String {
    // UUID 생성
    let uuid = Uuid::new_v4();

    // 나노초 단위의 타임스탬프
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // 주소, 체인ID, 타임스탬프, UUID, 논스를 모두 결합
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
