pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use crate::state::AppState;

use path::ChesterPath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            ChesterPath::Volume.as_str(),
            get(handler::get_volume),
        )
        .route(
            ChesterPath::Round.as_str(),
            get(handler::get_round),
        )
        .route(
            ChesterPath::Rewards.as_str(),
            get(handler::get_rewards),
        )
        .route(
            ChesterPath::SwapHistory.as_str(),
            get(handler::get_swap_history),
        )
}
