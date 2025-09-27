pub mod handler;
pub mod path;

use axum::{routing::get, Router};

use path::MetricsPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(MetricsPath::Metrics.as_str(), get(handler::get_metrics))
}