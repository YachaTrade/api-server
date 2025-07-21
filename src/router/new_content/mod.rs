pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{routing::get, Router};
use path::NewContentPath;

pub fn router() -> Router<AppState> {
    Router::new().route(
        NewContentPath::NewContent.as_str(),
        get(handler::get_new_content),
    )
}