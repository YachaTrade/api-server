pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};
use path::TerminalPath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            TerminalPath::LatestBlock.as_str(),
            get(handler::get_latest_block),
        )
        .route(TerminalPath::Asset.as_str(), get(handler::get_asset))
        .route(TerminalPath::Pair.as_str(), get(handler::get_pair))
        .route(TerminalPath::Events.as_str(), get(handler::get_events))
        .route(
            TerminalPath::GetMetadata.as_str(),
            get(handler::get_terminal_metadata),
        )
}
