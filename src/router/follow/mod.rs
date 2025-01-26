use axum::{
    routing::{get, put},
    Router,
};

use path::FollowPath;

use crate::state::AppState;

pub mod handler;
pub mod path;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(FollowPath::AddFollow.as_str(), put(handler::add_follow))
        .route(
            FollowPath::RemoveFollow.as_str(),
            put(handler::remove_follow),
        )
        .route(
            FollowPath::GetFollowers.as_str(),
            get(handler::get_followers),
        )
        .route(
            FollowPath::GetFollowings.as_str(),
            get(handler::get_followings),
        )
}
