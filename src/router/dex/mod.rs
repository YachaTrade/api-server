pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::DexPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(DexPath::GetPositions.as_str(), get(handler::get_positions))
}
