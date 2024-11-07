use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::ToSchema;

use crate::{
    db::postgres::{controller::follow::FollowController, model::Account},
    result::{AppError, AppJsonResult},
    state::AppState,
};

use super::path::Path;

#[derive(Debug, Deserialize, ToSchema)]
pub struct FollowRequest {
    #[schema(example = "follower targer address")]
    follower: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct FollowResponse {
    follower: Account,
    following: Account,
}

#[utoipa::path(
    put,
    path = Path::AddFollow.as_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = FollowRequest,
    responses(
        (status = 200, description = "Follow added successfully", body = FollowResponse),
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
    Json(payload): Json<FollowRequest>,
) -> AppJsonResult<FollowResponse> {
    // 페이로드의 주소와 세션 주소가 일치하는지 확인
    let FollowRequest { follower } = payload;
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

    Ok(Json(FollowResponse {
        follower: follower_account,
        following: following_account,
    }))
}

#[utoipa::path(
    put,
    path = "/follow/remove",
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = RemoveFollowRequest,
    responses(
        (status = 200, description = "Follow removed successfully", body = FollowResponse),
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
    Json(payload): Json<FollowRequest>,
) -> AppJsonResult<FollowResponse> {
    let FollowRequest { follower } = payload;
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

    Ok(Json(FollowResponse {
        follower: follower_account,
        following: following_account,
    }))
}
