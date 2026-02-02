use std::env;

use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::{
    Extension, Json,
    extract::State,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use tower_cookies::Cookie;
use tower_cookies::cookie::time::Duration;

use tracing::instrument;

use crate::middleware::SessionInfo;
use crate::router::auth::path::AuthPath;
use crate::services::auth::AuthService;
use crate::types::auth::{AuthNonceRequest, AuthNonceResponse, AuthSessionRequest};
use crate::{
    result::{AppError, AppJsonResult, AppResult},
    state::AppState,
};

/// Generate authentication nonce
#[utoipa::path(
    post,
    path = AuthPath::Nonce.as_str(),
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
    payload.validate().map_err(AppError::BadRequest)?;

    let service = AuthService::new(state.postgres.clone(), state.redis.clone());
    let response = service.generate_nonce(payload).await?;

    Ok(Json(response))
}

/// Generate authentication session
#[utoipa::path(
    post,
    path = AuthPath::Session.as_str(),
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
    payload.validate().map_err(AppError::BadRequest)?;

    let service = AuthService::new(state.postgres.clone(), state.redis.clone());
    let (response, session_id) = service.create_session(payload).await?;

    let cookie_name = env::var("COOKIE_NAME")
        .map_err(|_| AppError::InternalError("COOKIE_NAME not configured".to_string()))?;
    // 쿠키 설정
    let mut cookie = Cookie::new(cookie_name, session_id);
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_path("/");

    // cross-site 쿠키 설정 (nadapp.net → nad.fun)
    // domain 설정 없음 = nadapp.net
    // SameSite::None = cross-site 요청에서도 쿠키 전송
    cookie.set_same_site(tower_cookies::cookie::SameSite::None);

    cookie.set_max_age(Duration::hours(24));

    let body = Json(response);
    let response = Response::builder()
        .header(
            SET_COOKIE,
            HeaderValue::from_str(&cookie.to_string())
                .map_err(|e| AppError::InternalError(e.to_string()))?,
        )
        .body(body.into_response())
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    Ok(response)
}

/// Delete authentication session
#[utoipa::path(
    delete,
    path = AuthPath::DeleteSession.as_str(),
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
    Extension(session_info): Extension<SessionInfo>,
) -> AppResult<impl IntoResponse> {
    let service = AuthService::new(state.postgres.clone(), state.redis.clone());
    service.delete_session(&session_info.session_id).await?;

    // Remove session cookie by setting its expiry to a past date
    let cookie_name = env::var("COOKIE_NAME").unwrap_or_else(|_| "nadfun-v3-api".to_string());
    let mut cookie = Cookie::new(cookie_name, "");
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_path("/");

    // cross-site 쿠키 설정 (nadapp.net → nad.fun)
    // domain 설정 없음 = nadapp.net
    // SameSite::None = cross-site 요청에서도 쿠키 전송
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
