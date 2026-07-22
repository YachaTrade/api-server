use axum::{
    Extension, Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::AnalyticsPath;
use crate::{
    controllers::cms::analytics::AnalyticsController,
    result::{AppError, AppJsonResult},
    state::AppState,
    types::cms::analytics::{
        NewUsersQuery, NewUsersResponse, UserActivityQuery, UserActivityResponse, UserRoiResponse,
    },
};

/// Get churned users analytics (Admin only)
#[utoipa::path(
    get,
    path = AnalyticsPath::ChurnedUsers.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)"),
        UserActivityQuery,
    ),
    responses(
        (status = 200, description = "Churned users analytics", body = UserActivityResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS Analytics"
)]
#[instrument(skip(state))]
pub async fn get_churned_users(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<UserActivityQuery>,
) -> AppJsonResult<UserActivityResponse> {
    verify_admin(&state, &session_address).await?;

    if query.inactive_days <= 0 {
        return Err(AppError::BadRequest(
            "inactive_days must be positive".to_string(),
        ));
    }

    let limit = query.limit.unwrap_or(10).min(100);
    let controller = AnalyticsController::new(state.postgres.clone());
    let response = controller
        .get_user_activity(query.inactive_days, limit, true)
        .await
        .map_err(|err| {
            AppError::InternalError(format!("Failed to fetch churned users: {}", err))
        })?;

    Ok(Json(response))
}

/// Get active users analytics (Admin only)
#[utoipa::path(
    get,
    path = AnalyticsPath::ActiveUsers.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)"),
        UserActivityQuery,
    ),
    responses(
        (status = 200, description = "Active users analytics", body = UserActivityResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS Analytics"
)]
#[instrument(skip(state))]
pub async fn get_active_users(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<UserActivityQuery>,
) -> AppJsonResult<UserActivityResponse> {
    verify_admin(&state, &session_address).await?;

    if query.inactive_days <= 0 {
        return Err(AppError::BadRequest(
            "inactive_days must be positive".to_string(),
        ));
    }

    let limit = query.limit.unwrap_or(10).min(100);
    let controller = AnalyticsController::new(state.postgres.clone());
    let response = controller
        .get_user_activity(query.inactive_days, limit, false)
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to fetch active users: {}", err)))?;

    Ok(Json(response))
}

/// Get new users count (Admin only)
#[utoipa::path(
    get,
    path = AnalyticsPath::NewUsers.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)"),
        NewUsersQuery,
    ),
    responses(
        (status = 200, description = "New users count", body = NewUsersResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS Analytics"
)]
#[instrument(skip(state))]
pub async fn get_new_users(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<NewUsersQuery>,
) -> AppJsonResult<NewUsersResponse> {
    verify_admin(&state, &session_address).await?;

    if query.days <= 0 {
        return Err(AppError::BadRequest("days must be positive".to_string()));
    }

    let controller = AnalyticsController::new(state.postgres.clone());
    let response = controller
        .get_new_users(query.days)
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to fetch new users: {}", err)))?;

    Ok(Json(response))
}

/// Get user ROI statistics (Admin only)
#[utoipa::path(
    get,
    path = AnalyticsPath::UserRoi.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)")
    ),
    responses(
        (status = 200, description = "User ROI statistics", body = UserRoiResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS Analytics"
)]
#[instrument(skip(state))]
pub async fn get_user_roi(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<UserRoiResponse> {
    verify_admin(&state, &session_address).await?;

    let controller = AnalyticsController::new(state.postgres.clone());
    let response = controller
        .get_user_roi()
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to fetch user ROI: {}", err)))?;

    Ok(Json(response))
}

/// Admin 권한 확인 헬퍼
async fn verify_admin(state: &AppState, session_address: &str) -> Result<(), AppError> {
    let cms_controller = crate::controllers::cms::CmsController::new(state.postgres.clone());
    let is_admin = cms_controller
        .verify_admin(session_address)
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to verify admin: {}", err)))?;

    if !is_admin {
        return Err(AppError::AuthError("Admin access required".to_string()));
    }
    Ok(())
}
