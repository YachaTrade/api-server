use crate::db::postgres::controller::session::SessionController;

use super::{constant::EXPIRATION_SESSION_KEY, result::AppError, state::AppState};

use axum::{
    body::Body,
    extract::State,
    http::{Request, Response},
    middleware::Next,
};
use tower_cookies::Cookies;
use tracing::info;

// pub async fn authenticate_user(
//     State(state): State<AppState>,
//     cookies: Cookies,
//     mut req: Request<Body>,
//     next: Next,
// ) -> Result<Response<Body>, AppError> {
//     // info!("Header ={:#?}", req.headers());
//     let session_key = match cookies.get("session") {
//         Some(cookie) => cookie.value().to_string(),
//         None => return Err(AppError::AuthError("Session cookie is missing".to_string())),
//     };

//     info!("Session key: {}", session_key);
//     let redis = state.redis.clone();
//     let postgres = state.postgres.clone();

//     let session_address = match redis.get_address_by_session(&session_key).await {
//         Ok(address) => address,
//         Err(_) => {
//             // info!("Session key not found in Redis");
//             // Redis에 없다면 PostgreSQL에서 찾기
//             let session_controller = SessionController::new(postgres.clone());
//             let address = session_controller
//                 .get_address_by_session_id(&session_key)
//                 .await
//                 .map_err(|_| AppError::AuthError("Invalid session key".to_string()))?;

//             // PostgreSQL에서 찾은 주소를 Redis에 추가
//             redis
//                 .set_session(&session_key, &address, *EXPIRATION_SESSION_KEY)
//                 .await
//                 .map_err(|_| AppError::InternalError("Session Save is Error".into()))?; // 여기를 수정했습니다.

//             address
//         }
//     };
//     info!("Pass authentication {:?}", session_address);
//     // 요청에 인증된 사용자 주소 추가
//     req.extensions_mut().insert(session_address);

//     // 다음 핸들러로 요청 전달
//     Ok(next.run(req).await)
// }
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

    info!("Session key: {}", session_key);
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
    info!("Pass authentication {:?}", session_address);
    req.extensions_mut().insert(session_address);

    Ok(next.run(req).await)
}
