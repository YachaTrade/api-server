pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};

use path::RafflePath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(RafflePath::GetEligible.as_str(), get(handler::get_eligible))
        .route(RafflePath::GetPrizes.as_str(), get(handler::get_prizes))
}
