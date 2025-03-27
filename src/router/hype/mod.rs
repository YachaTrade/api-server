pub mod path;

pub mod handler;
use axum::{routing::get, Router};

use path::HypePath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(HypePath::GetHype.as_str(), get(handler::get_hype_token))
        .route(HypePath::GetHonor.as_str(), get(handler::get_honor_token))
}
