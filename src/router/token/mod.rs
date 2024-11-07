pub mod path;

pub mod handler;
use axum::{routing::put, Router};
use handler::update_token;
use path::Path;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(Path::UpdateToken.as_str(), put(update_token))
}
