pub mod handler;

use axum::{Router, routing::get};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(handler::health_check))
}
