use crate::controllers::auth::session::SessionController;
use crate::services::api_key::{update_last_used, validate_api_key};
use crate::services::rate_limiter::{
    RATE_LIMIT_WITH_API_KEY, RATE_LIMIT_WITHOUT_API_KEY, RateLimitResult, check_and_increment,
};

use super::{config::EXPIRATION_SESSION_KEY, result::AppError, state::AppState};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, Response, header::ORIGIN},
    middleware::Next,
};
use std::env;
use std::net::SocketAddr;
use tower_cookies::Cookies;
use tracing::{error, info, warn};

/// 허용된 Origin인지 검증 (CSRF 방어).
/// CORS와 동일한 정책을 사용해 인증/CSRF/API-key 우회가 달라지지 않게 한다.
fn is_allowed_origin(origin: &str) -> bool {
    crate::cors::is_origin_allowed(origin)
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub session_id: String,
    pub address: String,
}

/// 쿠키 → 세션 키 → 주소 해석 (Redis 우선, Postgres 폴백 + 재캐싱).
/// CSRF Origin 검증은 포함하지 않음 — 호출자가 필요에 따라 별도로 수행한다.
pub async fn resolve_session_address(
    state: &AppState,
    cookies: &Cookies,
) -> Result<(String, String), AppError> {
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

    Ok((session_key, session_address))
}

/// 세션 쿠키가 있으면 주소를 반환, 없거나 유효하지 않으면 `None`.
/// CSRF Origin 검증 없이 공개 GET 핸들러의 선택적 개인화(`liked_by_me` 등)에 사용.
pub async fn optional_session_address(state: &AppState, cookies: &Cookies) -> Option<String> {
    resolve_session_address(state, cookies)
        .await
        .ok()
        .map(|(_, address)| address)
}

pub async fn authenticate_user(
    State(state): State<AppState>,
    cookies: Cookies,
    mut req: Request<Body>, // 구체적인 Body 타입 사용
    next: Next,             // Body 타입 명시
) -> Result<Response<Body>, AppError> {
    info!(
        "[AUTH] authenticate_user called, path: {}",
        req.uri().path()
    );

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

    let (session_key, session_address) = resolve_session_address(&state, &cookies).await?;

    let session_info = SessionInfo {
        session_id: session_key,
        address: session_address.clone(),
    };
    req.extensions_mut().insert(session_info);
    req.extensions_mut().insert(session_address);

    Ok(next.run(req).await)
}

/// Terminal 메타데이터 라우트는 루트 레벨 단일 세그먼트 토큰 주소다
/// (`TerminalPath::GetMetadata` = `/:token_address`). 다른 terminal
/// (GeckoTerminal) 엔드포인트처럼 API Key 게이트를 우회해야 하지만 리터럴
/// 경로로 나열할 수 없어 형태로 매칭한다: `/0x` + hex 40자리, 추가 세그먼트
/// 없음. 루트 와일드카드 라우트는 axum에서 하나뿐이므로 이 형태는 terminal
/// 메타데이터(또는 404)로만 라우팅된다. 실제 토큰 존재/체크섬 검증은
/// 핸들러(`valid_existing_token_id`)가 한다.
fn is_terminal_metadata_path(path: &str) -> bool {
    path.strip_prefix("/0x")
        .is_some_and(|rest| rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// API Key 검증 및 Rate Limit 미들웨어 (전역 적용)
/// - Yacha 프론트엔드 및 로컬 개발 Origin: 통과
/// - 외부 Origin + API Key: 100 req/min
/// - 외부 Origin + No API Key: 10 req/min (IP 기반)
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
        || is_terminal_metadata_path(path)
    // `/:token_address` (terminal 메타데이터)
    {
        return Ok(next.run(req).await);
    }

    // 1. Origin 헤더 확인
    let origin = req.headers().get(ORIGIN).and_then(|v| v.to_str().ok());

    // 2. CORS 허용 Origin이면 API Key 검사 건너뛰기
    if let Some(origin_str) = origin {
        if is_allowed_origin(origin_str) {
            // Yacha 프론트엔드 및 로컬 개발 → API Key 불필요, Rate Limit 없음
            return Ok(next.run(req).await);
        }
    }

    // 3. 외부 Origin 또는 Origin 없음 → API Key 선택적 (Rate Limit 차등 적용)
    let api_key = req.headers().get("X-API-Key").and_then(|v| v.to_str().ok());

    // 4. IP 주소 추출 (API Key 없을 때 rate limit용)
    // Cloudflare -> HAProxy -> API Server 구조에서 원본 IP 추출
    let (client_ip, ip_source) = if let Some(ip) = req
        .headers()
        .get("CF-Connecting-IP")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
    {
        (ip, "CF-Connecting-IP")
    } else if let Some(ip) = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
    {
        (ip, "X-Forwarded-For")
    } else if let Some(ip) = req
        .headers()
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    {
        (ip, "X-Real-IP")
    } else if let Some(ip) = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
    {
        (ip, "ConnectInfo")
    } else {
        ("unknown".to_string(), "fallback")
    };

    info!(
        "[RATE_LIMIT] path={}, client_ip={}, source={}, api_key={}",
        path,
        client_ip,
        ip_source,
        api_key.map(|_| "present").unwrap_or("none")
    );

    let (rate_limit_id, rate_limit, has_api_key) = if let Some(key) = api_key {
        // 5a. API Key 있음 → 검증 후 100 req/min
        let key_info = validate_api_key(&state.postgres, &state.redis, key).await?;

        // last_used_at 업데이트 (Redis에 저장, 주기적으로 DB 동기화)
        let redis = state.redis.clone();
        let hash = key_info.key_hash.clone();
        tokio::spawn(async move {
            update_last_used(&redis, &hash).await;
        });

        (
            format!("key:{}", key_info.key_hash),
            RATE_LIMIT_WITH_API_KEY,
            true,
        )
    } else {
        // 5b. API Key 없음 → IP 기반 10 req/min
        (
            format!("ip:{}", client_ip),
            RATE_LIMIT_WITHOUT_API_KEY,
            false,
        )
    };

    // 6. Rate Limit 확인
    match check_and_increment(&state.redis, &rate_limit_id, rate_limit).await? {
        RateLimitResult::Exceeded { retry_after, limit } => {
            warn!(
                "[RATE_LIMIT_BLOCKED] path={}, client_ip={}, identifier={}, limit={}, retry_after={}s, has_api_key={}",
                path, client_ip, rate_limit_id, limit, retry_after, has_api_key
            );
            return Err(AppError::TooManyRequests { retry_after });
        }
        RateLimitResult::Allowed { remaining, limit } => {
            // 7. 요청 처리
            let mut response = next.run(req).await;

            // 8. Rate Limit 헤더 추가
            response
                .headers_mut()
                .insert("X-RateLimit-Limit", limit.to_string().parse().unwrap());
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            response
                .headers_mut()
                .insert("X-RateLimit-Window", "1m".parse().unwrap());

            if !has_api_key {
                response.headers_mut().insert(
                    "X-RateLimit-Upgrade",
                    "Get API key for 100 req/min".parse().unwrap(),
                );
            }

            return Ok(response);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_allowed_origin, is_terminal_metadata_path};

    #[test]
    fn terminal_metadata_path_shape() {
        // 정확히 /0x + hex 40자리만 통과
        assert!(is_terminal_metadata_path(
            "/0x5dF178C7E58046BC9074782fef0009C6Be167777"
        ));
        assert!(is_terminal_metadata_path(
            "/0x0000000000000000000000000000000000007777"
        ));
        // 거부: 길이 미달/초과, 비-hex, 추가 세그먼트, 무관한 경로
        assert!(!is_terminal_metadata_path("/0x1234"));
        assert!(!is_terminal_metadata_path(
            "/0x5dF178C7E58046BC9074782fef0009C6Be16777" // 39자리
        ));
        assert!(!is_terminal_metadata_path(
            "/0x5dF178C7E58046BC9074782fef0009C6Be1677777" // 41자리
        ));
        assert!(!is_terminal_metadata_path(
            "/0xZZF178C7E58046BC9074782fef0009C6Be167777" // 비-hex
        ));
        assert!(!is_terminal_metadata_path(
            "/0x5dF178C7E58046BC9074782fef0009C6Be167777/extra"
        ));
        assert!(!is_terminal_metadata_path("/unrelated"));
        assert!(!is_terminal_metadata_path("/"));
    }

    #[test]
    fn yacha_and_local_origins_only() {
        assert!(is_allowed_origin("https://app.yacha.trade"));
        assert!(is_allowed_origin("https://dev.yacha.trade"));
        assert!(is_allowed_origin("https://a.b.yacha.trade"));
        assert!(is_allowed_origin("http://localhost:3000"));
        assert!(!is_allowed_origin("https://yacha.trade"));
        assert!(!is_allowed_origin("http://dev.yacha.trade"));
        assert!(!is_allowed_origin("https://evil-yacha.trade"));
        assert!(!is_allowed_origin("https://yacha.trade.evil.com"));
    }
}
