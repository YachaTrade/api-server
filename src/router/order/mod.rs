pub mod handler;
pub mod path;
use crate::state::AppState;

use axum::{Router, routing::get};

use path::OrderPath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            OrderPath::Creationtime.as_str(),
            get(handler::get_creation_time_order),
        )
        .route(
            OrderPath::MarketCap.as_str(),
            get(handler::get_market_cap_order),
        )
        .route(
            OrderPath::LatestTrade.as_str(),
            get(handler::get_latest_trade_order),
        )
}
