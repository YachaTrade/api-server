pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};
use path::NewEventPath;

pub fn router() -> Router<AppState> {
    Router::new().route(NewEventPath::NewEvent.as_str(), get(handler::get_new_event))
}
