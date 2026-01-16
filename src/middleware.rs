use crate::controllers::auth::session::SessionController;

use super::{config::EXPIRATION_SESSION_KEY, result::AppError, state::AppState};

use axum::{
    body::Body,
    extract::State,
    http::{header::ORIGIN, Request, Response},
    middleware::Next,
};
use std::env;
use tower_cookies::Cookies;
use tracing::error;

/// 허용된 Origin인지 검증
fn is_allowed_origin(origin: &str) -> bool {
    let is_dev = env::var("ENVIRONMENT")
        .map(|e| e == "DEV")
        .unwrap_or(false);

    origin == "https://nad.fun"
        || origin == "https://nadapp.net"
        || origin.ends_with(".nad.fun")
        || origin.ends_with(".symphony.io")
        || (is_dev && origin.starts_with("http://localhost:"))
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
    // Origin 헤더 검증 (CSRF 방어)
    if let Some(origin) = req.headers().get(ORIGIN) {
        let origin_str = origin.to_str().map_err(|_| {
            error!("Invalid Origin header: {:?}", origin);
            AppError::AuthError("Invalid Origin header".to_string())
        })?;
        if !is_allowed_origin(origin_str) {
            error!("Origin not allowed: {}", origin_str);
            return Err(AppError::AuthError("Origin not allowed".to_string()));
        }
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
