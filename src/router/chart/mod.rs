use axum::{routing::get, Router};

use crate::state::AppState;

pub mod handler;
pub mod path;

pub fn router() -> Router<AppState> {
    Router::new().route(path::ChartPath::GetChart.as_str(), get(handler::get_chart))
}
