use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};

use tracing::{info, instrument};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        social::follow::{
            FollowController, FollowsResponse, UpdateFollowRequest, UpdateFollowResponse,
            CheckFollowResponse,
        },
    },
};

use super::path::FollowPath;

#[utoipa::path(
    put,
    path = FollowPath::AddFollow.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = UpdateFollowRequest,
    responses(
        (status = 200, description = "Follow added successfully", body = UpdateFollowResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
     tag = "Follow"
)]
#[instrument(skip(state))]
pub async fn add_follow(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateFollowRequest>,
) -> AppJsonResult<UpdateFollowResponse> {
    // 페이로드의 주소와 세션 주소가 일치하는지 확인
    let UpdateFollowRequest { follower } = payload;
    let following = session_address;
    if following == follower {
        return Err(AppError::BadRequest(
            "Address and target_address are same".into(),
        ));
    }

    let follow_controller = FollowController::new(state.postgres.clone());

    let (follower_account, following_account) = follow_controller
        .add_follow(follower, following)
        .await
        .map_err(|err| {
            info!("Add Follow Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(UpdateFollowResponse {
        follower: follower_account,
        following: following_account,
    }))
}

#[utoipa::path(
    put,
    path = FollowPath::RemoveFollow.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = UpdateFollowRequest,
    responses(
        (status = 200, description = "Follow removed successfully", body = UpdateFollowResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
     tag = "Follow"
)]
pub async fn remove_follow(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateFollowRequest>,
) -> AppJsonResult<UpdateFollowResponse> {
    let UpdateFollowRequest { follower } = payload;
    let following = session_address;

    if following == follower {
        return Err(AppError::BadRequest(
            "Address and target_address are same".into(),
        ));
    }
    let follow_controller = FollowController::new(state.postgres.clone());
    let (follower_account, following_account) = follow_controller
        .remove_follow(follower, following)
        .await
        .map_err(|err| {
            info!("Add Follow Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(UpdateFollowResponse {
        follower: follower_account,
        following: following_account,
    }))
}

#[utoipa::path(
    get,
    path = FollowPath::CheckFollow.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to check follow status for"),
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Follow status checked successfully", body = CheckFollowResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Follow"
)]
#[instrument(skip(state))]
pub async fn check_follow(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(account_id): Path<String>,
) -> AppJsonResult<CheckFollowResponse> {
    let follow_controller = FollowController::new(state.postgres.clone());
    
    let is_following = follow_controller
        .check_follow(session_address, account_id)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(CheckFollowResponse { is_following }))
}

/// Get followers with pagination
#[utoipa::path(
    get,
    path = FollowPath::GetFollowers.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get followers for"),
        ("page" = i32, Query, description = "Page number (starts from 1)"),
        ("limit" = i32, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved followers", body = FollowsResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_followers(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<FollowsResponse> {
    let follows = FollowController::new(state.postgres.clone())
        .get_follows(&account_id, false, pagination)
        .await?;
    Ok(Json(FollowsResponse { accounts: follows }))
}

/// Get following accounts with pagination
#[utoipa::path(
    get,
    path = FollowPath::GetFollowings.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account ID to get following accounts for"),
        ("page" = i32, Query, description = "Page number (starts from 1)"),
        ("limit" = i32, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved following accounts", body = FollowsResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Profile"
)]
pub async fn get_followings(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<FollowsResponse> {
    let follows = FollowController::new(state.postgres.clone())
        .get_follows(&account_id, true, pagination)
        .await?;
    Ok(Json(FollowsResponse { accounts: follows }))
}
