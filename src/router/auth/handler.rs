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
    let time_start = std::time::Instant::now();
    let nonce = Uuid::new_v4().to_string();
    match Address::from_str(&payload.address) {
        Ok(_) => (),
        Err(e) => {
            error!("Invalid address: {}", e);
            return Err(AppError::BadRequest("Invalid address".to_string()).into());
        }
    }

    let redis = state.session_redis.clone();
    if let Err(err) = redis.set_nonce(&payload.address, &nonce).await {
        error!("Failed to set nonce: {}", err);
        return Err(AppError::RedisError(err.to_string()).into());
    }

    let time_end = time_start.elapsed();
    info!("auth_nonce time: {:?}ms", time_end.as_millis());
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
    let time_start = std::time::Instant::now();

    let chain_id = payload.chain_id;
    let env_chain_id = env::var("CHAIN_ID")
        .expect("CHAIN_ID must be set")
        .parse::<u64>()
        .unwrap();

    if chain_id != env_chain_id {
        error!(
            "Invalid chain ID. Monad test chain Id is {} your chain Id is {}",
            env_chain_id, chain_id
        );
        return Err(AppError::BadRequest(format!(
            "Invalid chain ID. Monad test chain Id is {} your chain Id is {}",
            env_chain_id, chain_id
        ))
        .into());
    }

    // 1. 주소와 서명 파싱
    let nonce = payload.nonce;
    let signature = Signature::from_str(&payload.signature).map_err(|err| {
        error!("Invalid signature format: {}", err);
        AppError::BadRequest("Invalid signature format".to_string())
    })?;

    // 2. 서명에서 주소 복구
    let address = signature
        .recover_address_from_msg(nonce.clone())
        .map_err(|_| {
            error!("Failed to recover address: {}", nonce);
            AppError::Unauthorized("Failed to recover address".to_string())
        })?
        .to_string();

    let redis = state.session_redis.clone();

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

    let session_id = generate_session_id(address.as_str(), nonce.as_str());

    // 병렬로 Redis 작업 실행

    let (del_nonce_result, set_session_result, postgres_set_session_result) = tokio::join!(
        {
            let start = std::time::Instant::now();
            let result = redis.del_nonce(&address);
            let elapsed = start.elapsed();
            info!("del_nonce elapsed: {:?}", elapsed);
            result
        },
        {
            let start = std::time::Instant::now();
            let result = redis.set_session(&session_id, &address, *EXPIRATION_SESSION_KEY);
            let elapsed = start.elapsed();
            info!("set_session elapsed: {:?}", elapsed);
            result
        },
        {
            let start = std::time::Instant::now();
            let postgres = state.postgres.clone();
            let session_id = session_id.clone();
            let address = address.clone();
            async move {
                let session_controller = SessionController::new(postgres);
                let result = session_controller.set_session(&session_id, &address).await;
                let elapsed = start.elapsed();
                info!("postgres_set_session elapsed: {:?}", elapsed);
                result
            }
        }
    );

    // 각 결과 확인
    del_nonce_result.map_err(|err| {
        error!(
            "Failed to delete nonce: address: {}, error: {}",
            address, err
        );
        AppError::RedisError(err.to_string())
    })?;

    set_session_result.map_err(|err| {
        error!(
            "Failed to set session: session_id: {}, address: {}, error: {}",
            session_id, address, err
        );
        AppError::RedisError(err.to_string())
    })?;

    postgres_set_session_result.map_err(|err| {
        error!(
            "Failed to set session in Postgres: session_id: {}, address: {}, error: {}",
            session_id, address, err
        );
        AppError::InternalError(err.to_string())
    })?;

    // 이전 작업들이 모두 성공한 후 계정 업서트 수행
    let postgres = state.postgres.clone();
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

    // 쿠키 설정
    let mut cookie = Cookie::new("api-session", session_id);
    cookie.set_http_only(true);
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
    info!(
        "auth session success {}ms",
        time_start.elapsed().as_millis()
    );
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
    let time_start = std::time::Instant::now();
    // 병렬로 Redis와 PostgreSQL에서 세션 정보 삭제
    let postgres_clone = state.postgres.clone();
    let redis_clone = state.session_redis.clone();
    let session_controller = SessionController::new(postgres_clone);
    let start_time = std::time::Instant::now();
    let (redis_result, postgres_result) = tokio::join!(
        {
            let start = std::time::Instant::now();
            let result = redis_clone.delete_session(&session_address);
            let elapsed = start.elapsed();
            info!("Redis delete_session elapsed: {:?}", elapsed);
            result
        },
        {
            let start = std::time::Instant::now();
            let result = session_controller.delete_session_by_id(&session_address);
            let elapsed = start.elapsed();
            info!("PostgreSQL delete_session elapsed: {:?}", elapsed);
            result
        }
    );
    let elapsed = start_time.elapsed();
    // PostgreSQL 결과 처리
    postgres_result.map_err(|err| {
        error!(
            "Failed to delete session from PostgreSQL: session_address: {}, error: {}, elapsed: {:?}",
            session_address, err, elapsed
        );
        AppError::InternalError(err.to_string())
    })?;

    info!("Session deletion completed in {:?}", elapsed);

    // Redis 결과 처리
    redis_result.map_err(|err| {
        error!(
            "Failed to delete session from Redis: session_address: {}, error: {}",
            session_address, err
        );
        AppError::RedisError(err.to_string())
    })?;

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
    info!(
        "Delete session success {}ms",
        time_start.elapsed().as_millis()
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
