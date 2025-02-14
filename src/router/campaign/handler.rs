use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        campaign::{
            active::{ActiveUserController, ActiveUserResponse},
            point::{AccountPointResponse, PointController, TopPointResponse},
        },
        common::pagination::PaginationParams,
    },
    utils::valid_evm_address,
};

use super::path::{ActiveUserPath, ScorePath};
//------------------------------------------------------------Active User------------------------------------------------------------//
/// Check if a user is active
#[utoipa::path(
    get,
    path = ActiveUserPath::CheckActiveUser.docs_str(),
    params(
        ("wallet_address" = String, Path, description = "Ethereum wallet address to check")
    ),
    responses(
        (status = 200, description = "Successfully retrieved active user status", body = ActiveUserResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign"
)]
pub async fn check_active_user(
    Path(wallet_address): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<ActiveUserResponse> {
    if !valid_evm_address(&wallet_address) {
        return Err(AppError::BadRequest("Invalid wallet address".to_string()));
    }
    let response = ActiveUserController::new(state.postgres.clone())
        .check_active_user(&wallet_address)
        .await?;
    Ok(Json(response))
}

//------------------------------------------------------------Score------------------------------------------------------------//
/// Get top point rankings with pagination
#[utoipa::path(
    get,
    path = ScorePath::GetTop.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items limit")
    ),
    responses(
        (status = 200, description = "Successfully retrieved top point rankings", body = TopPointResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign"
)]
pub async fn get_top_point(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<TopPointResponse> {
    let response = PointController::new(state.postgres.clone())
        .get_top_point(params)
        .await?;

    Ok(Json(response))
}
///Get account point by account id
#[utoipa::path(
    get,
    path = ScorePath::GetAccountPoint.docs_str(),
    responses(
        (status = 200, description = "Successfully retrieved account point", body = AccountPointResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign",
    security(
        ("session_token" = [])
    )
)]
pub async fn get_score_by_account_id(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountPointResponse> {
    let response = PointController::new(state.postgres.clone())
        .get_account_point_rank(session_address)
        .await?;
    Ok(Json(response))
}
