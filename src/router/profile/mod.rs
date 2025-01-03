pub mod handler;
pub mod path;
use crate::state::AppState;

use axum::{routing::get, Router};

use path::ProfilePath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(ProfilePath::Profile.as_str(), get(handler::get_profile))
        .route(
            ProfilePath::TokenCreated.as_str(),
            get(handler::get_created_tokens),
        )
        .route(
            ProfilePath::TokenHeld.as_str(),
            get(handler::get_tokens_held),
        )
        .route(ProfilePath::Replies.as_str(), get(handler::get_replies))
        .route(ProfilePath::Followers.as_str(), get(handler::get_followers))
        .route(ProfilePath::Following.as_str(), get(handler::get_following))
}
