pub mod handler;
pub mod path;
use crate::state::AppState;

use axum::{routing::get, Router};

use path::ProfilePath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(ProfilePath::GetProfile.as_str(), get(handler::get_profile))
        .route(ProfilePath::GetPnl.as_str(), get(handler::get_pnl))
        .route(
            ProfilePath::GetPosition.as_str(),
            get(handler::get_position),
        )
        .route(
            ProfilePath::GetTokenCreated.as_str(),
            get(handler::get_token_created),
        )
        .route(
            ProfilePath::GetSwapHistory.as_str(),
            get(handler::get_swap_history),
        )
}
