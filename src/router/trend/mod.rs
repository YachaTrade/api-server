pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};
use path::TrendPath;

pub fn router(_state: AppState) -> Router<AppState> {
    Router::new().route(TrendPath::GetTrend.as_str(), get(handler::get_trend))
}
