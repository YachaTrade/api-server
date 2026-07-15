use anyhow::Error;
use axum::{
    Json,
    http::{Response, StatusCode},
    response::IntoResponse,
};

use redis::RedisError;
use serde_json::json;
use tracing::error;

pub type AppResult<T> = Result<T, AppError>;
pub type AppJsonResult<T> = AppResult<Json<T>>;
pub type AppResonseResult<T> = AppResult<Response<T>>;
/// From: https://github.com/Brendonovich/prisma-client-rust/blob/e520c5f6e30c0839d9dbccaa228f3eedbf188b6c/examples/axum-rest/src/routes.rs#L118
#[derive(Debug)]
pub enum AppError {
    AnyhowError(anyhow::Error),
    RouteError(String),
    RedisError(String),
    Unauthorized(String),
    AuthError(String),
    BadRequest(String),
    PayloadTooLarge(String),
    Forbidden(String),
    Gone(String),
    NotFound(String),
    InternalError(String),
    Conflict(String),
    ServiceUnavailable(String),
    TooManyRequests { retry_after: u64 },
}

impl From<RedisError> for AppError {
    fn from(error: RedisError) -> Self {
        AppError::RedisError(error.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: Error) -> Self {
        AppError::AnyhowError(error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response<axum::body::Body> {
        let (status, error_message) = match self {
            AppError::RouteError(err) => (StatusCode::BAD_REQUEST, err),
            AppError::AnyhowError(err) => {
                error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::RedisError(err) => {
                error!("Redis error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            AppError::AuthError(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Gone(msg) => (StatusCode::GONE, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::InternalError(msg) => {
                error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::TooManyRequests { retry_after } => {
                // 429 응답에 Retry-After 헤더 포함
                let body = Json(json!({
                    "error": "Rate limit exceeded",
                    "retry_after": retry_after
                }));
                let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                response
                    .headers_mut()
                    .insert("Retry-After", retry_after.to_string().parse().unwrap());
                return response;
            }
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod error_status_tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn forbidden_and_gone_map_to_403_and_410() {
        assert_eq!(
            AppError::Forbidden("creator_mismatch".into())
                .into_response()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::Gone("verification_expired".into())
                .into_response()
                .status(),
            StatusCode::GONE
        );
    }

    #[test]
    fn conflict_and_service_unavailable_map_to_409_and_503() {
        assert_eq!(
            AppError::Conflict("already_deployed".into())
                .into_response()
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::ServiceUnavailable("onchain_check_unavailable".into())
                .into_response()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn payload_too_large_maps_to_413() {
        assert_eq!(
            AppError::PayloadTooLarge("too large".into())
                .into_response()
                .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
