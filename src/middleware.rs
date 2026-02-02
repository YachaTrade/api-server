use crate::controllers::auth::session::SessionController;
use crate::services::api_key::{update_last_used, validate_api_key};
use crate::services::rate_limiter::{check_and_increment, RateLimitResult};

use super::{config::EXPIRATION_SESSION_KEY, result::AppError, state::AppState};

use axum::{
    body::Body,
    extract::State,
    http::{header::ORIGIN, Request, Response},
    middleware::Next,
};
use std::env;
use tower_cookies::Cookies;
use tracing::{error, info};

/// 허용된 Origin인지 검증
fn is_allowed_origin(origin: &str) -> bool {
    origin == "https://nad.fun"
        || origin == "https://nadapp.net"
        || origin.ends_with(".nad.fun")
        || origin.ends_with(".symphony.io")
        || origin.starts_with("http://localhost:")
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub session_id: String,
    pub address: String,
}

pub async fn authenticate_user(
    State(state): State<AppState>,
    cookies: Cookies,
    mut req: Request<Body>, // 구체적인 Body 타입 사용
    next: Next,             // Body 타입 명시
) -> Result<Response<Body>, AppError> {
    info!("[AUTH] authenticate_user called, path: {}", req.uri().path());

    // Origin 헤더 검증 (CSRF 방어)
    if let Some(origin) = req.headers().get(ORIGIN) {
        let origin_str = origin.to_str().map_err(|_| {
            error!("[AUTH] Invalid Origin header: {:?}", origin);
            AppError::AuthError("Invalid Origin header".to_string())
        })?;
        info!("[AUTH] Origin header: {}", origin_str);
        if !is_allowed_origin(origin_str) {
            error!("[AUTH] Origin not allowed: {}", origin_str);
            return Err(AppError::AuthError("Origin not allowed".to_string()));
        }
        info!("[AUTH] Origin allowed: {}", origin_str);
    } else {
        info!("[AUTH] No Origin header present");
    }

    let cookie_name =
        env::var("COOKIE_NAME").expect("COOKIE_NAME environment variable must be set");
    let session_key = match cookies.get(&cookie_name) {
        Some(cookie) => cookie.value().to_string(),
        None => return Err(AppError::AuthError("Session cookie is missing".to_string())),
    };

    let redis = state.redis.clone();
    let postgres = state.postgres.clone();

    let session_address = match redis.get_address_by_session(&session_key).await {
        Ok(address) => address,
        Err(_) => {
            let session_controller = SessionController::new(postgres.clone());
            let address = session_controller
                .get_address_by_session_id(&session_key)
                .await
                .map_err(|_| AppError::AuthError("Invalid session key".to_string()))?;

            redis
                .set_session(&session_key, &address, *EXPIRATION_SESSION_KEY)
                .await
                .map_err(|_| AppError::InternalError("Session Save is Error".into()))?;

            address
        }
    };

    let session_info = SessionInfo {
        session_id: session_key,
        address: session_address.clone(),
    };
    req.extensions_mut().insert(session_info);
    req.extensions_mut().insert(session_address);

    Ok(next.run(req).await)
}

/// API Key 검증 및 Rate Limit 미들웨어 (전역 적용)
/// - CORS 허용 Origin (nad.fun 등): 통과 (기존 동작)
/// - Origin 없음 또는 외부 Origin: X-API-Key 필수 + 1 req/sec
pub async fn api_key_gate(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, AppError> {
    // 0. 제외할 경로 확인
    let path = req.uri().path();
    if path == "/health"
        || path == "/"
        || path.starts_with("/cms/")
        || path.starts_with("/dev-sw")
        || path.starts_with("/api-key")  // API Key 관리 엔드포인트 (세션 인증 사용)
        || path.starts_with("/auth/")    // 인증 엔드포인트
        // Terminal (GeckoTerminal) 엔드포인트 - API Key 불필요
        || path == "/latest-block"
        || path == "/asset"
        || path == "/pair"
        || path == "/events"
    {
        return Ok(next.run(req).await);
    }

    // 1. Origin 헤더 확인
    let origin = req
        .headers()
        .get(ORIGIN)
        .and_then(|v| v.to_str().ok());

    // 2. CORS 허용 Origin이면 API Key 검사 건너뛰기
    if let Some(origin_str) = origin {
        if is_allowed_origin(origin_str) {
            // nad.fun, nadapp.net 등 → 기존 플로우 (API Key 불필요)
            return Ok(next.run(req).await);
        }
    }

    // 3. 외부 Origin 또는 Origin 없음 → API Key 필수
    let api_key = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("X-API-Key header required".to_string()))?;

    // 4. API Key 검증 (Redis 캐시 → DB)
    let key_info = validate_api_key(&state.postgres, &state.redis, api_key).await?;

    // 5. Rate Limit 확인 (60 req/min)
    match check_and_increment(&state.redis, &key_info.key_hash).await? {
        RateLimitResult::Exceeded { retry_after } => {
            return Err(AppError::TooManyRequests { retry_after });
        }
        RateLimitResult::Allowed { .. } => {}
    }

    // 6. last_used_at 업데이트 (비동기, fire-and-forget)
    let db = state.postgres.clone();
    let hash = key_info.key_hash.clone();
    tokio::spawn(async move {
        update_last_used(&db, &hash).await;
    });

    // 7. 요청 처리
    let mut response = next.run(req).await;

    // 8. Rate Limit 헤더 추가
    response
        .headers_mut()
        .insert("X-RateLimit-Limit", "60".parse().unwrap());
    response
        .headers_mut()
        .insert("X-RateLimit-Window", "1m".parse().unwrap());

    Ok(response)
}
