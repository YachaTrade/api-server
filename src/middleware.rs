use crate::types::auth::SessionController;

use super::{config::EXPIRATION_SESSION_KEY, result::AppError, state::AppState};

use axum::{
    body::Body,
    extract::State,
    http::{Request, Response},
    middleware::Next,
};
use tower_cookies::Cookies;

pub async fn authenticate_user(
    State(state): State<AppState>,
    cookies: Cookies,
    mut req: Request<Body>, // 구체적인 Body 타입 사용
    next: Next,             // Body 타입 명시
) -> Result<Response<Body>, AppError> {
    let session_key = match cookies.get("api-session") {
        Some(cookie) => cookie.value().to_string(),
        None => return Err(AppError::AuthError("Session cookie is missing".to_string())),
    };

    let session_redis = state.session_redis.clone();
    let postgres = state.postgres.clone();

    let session_address = match session_redis.get_address_by_session(&session_key).await {
        Ok(address) => address,
        Err(_) => {
            let session_controller = SessionController::new(postgres.clone());
            let address = session_controller
                .get_address_by_session_id(&session_key)
                .await
                .map_err(|_| AppError::AuthError("Invalid session key".to_string()))?;

            session_redis
                .set_session(&session_key, &address, *EXPIRATION_SESSION_KEY)
                .await
                .map_err(|_| AppError::InternalError("Session Save is Error".into()))?;

            address
        }
    };

    req.extensions_mut().insert(session_address);

    Ok(next.run(req).await)
}
