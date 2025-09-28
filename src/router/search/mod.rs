pub mod handler;
pub mod path;
use axum::{Router, routing::get};

use path::SearchPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(SearchPath::Search.as_str(), get(handler::search))
}
