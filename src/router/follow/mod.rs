use axum::{routing::put, Router};
use handler::{add_follow, remove_follow};
use path::Path;

use crate::state::AppState;

pub mod handler;
pub mod path;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(Path::AddFollow.as_str(), put(add_follow))
        .route(Path::RemoveFollow.as_str(), put(remove_follow))
}
