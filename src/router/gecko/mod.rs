pub mod path;
pub mod handler;

use axum::{Router, routing::get};
use path::GeckoPath;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(GeckoPath::LatestBlock.as_str(), get(handler::get_latest_block))
        .route(GeckoPath::Asset.as_str(), get(handler::get_asset))
        .route(GeckoPath::Pair.as_str(), get(handler::get_pair))
        .route(GeckoPath::Events.as_str(), get(handler::get_events))
        .route(GeckoPath::GetMetadata.as_str(), get(handler::get_gecko_metadata))
}
