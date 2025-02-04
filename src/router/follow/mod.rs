use axum::{
    middleware,
    routing::{delete, get, put},
    Router,
};

use path::FollowPath;

use crate::{middleware::authenticate_user, state::AppState};

pub mod handler;
pub mod path;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            FollowPath::AddFollow.as_str(),
            put(handler::add_follow).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            FollowPath::RemoveFollow.as_str(),
            delete(handler::remove_follow).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            FollowPath::CheckFollow.as_str(),
            get(handler::check_follow)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
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
