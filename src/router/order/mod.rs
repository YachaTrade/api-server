pub mod handler;
pub mod path;
use crate::state::AppState;

use axum::{routing::get, Router};

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
        .route(
            OrderPath::ReplyCount.as_str(),
            get(handler::get_reply_count_order),
        )
        .route(
            OrderPath::LatestReply.as_str(),
            get(handler::get_latest_reply_order),
        )
}
