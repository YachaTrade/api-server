use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};

use tracing::instrument;

use crate::{
    result::{AppError, AppJsonResult},
    services::social::SocialService,
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        social::follow::{
            CheckFollowResponse, FollowersResponse, FollowingResponse, UpdateFollowRequest,
            UpdateFollowResponse,
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

    let service = SocialService::new(state.postgres.clone());
    let response = service.add_follow(follower, following).await?;

    Ok(Json(response))
}

#[utoipa::path(
    delete,
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
#[instrument(skip(state))]
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
    let service = SocialService::new(state.postgres.clone());
    let response = service.remove_follow(follower, following).await?;

    Ok(Json(response))
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
    let service = SocialService::new(state.postgres.clone());

    let is_following = service.check_follow(account_id, session_address).await?;

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
        (status = 200, description = "Successfully retrieved followers", body = FollowersResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Follow"
)]
#[instrument(skip(state))]
pub async fn get_followers(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<FollowersResponse> {
    let service = SocialService::new(state.postgres.clone());
    let response = service.get_followers(&account_id, pagination).await?;
    Ok(Json(response))
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
        (status = 200, description = "Successfully retrieved following accounts", body = FollowingResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Follow"
)]
#[instrument(skip(state))]
pub async fn get_followings(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<FollowingResponse> {
    let service = SocialService::new(state.postgres.clone());
    let response = service.get_following(&account_id, pagination).await?;
    Ok(Json(response))
}
