pub mod handler;
pub mod path;
use axum::{routing::get, Router};

use handler::search_token;
use path::SearchPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(SearchPath::Search.as_str(), get(search_token))
}
