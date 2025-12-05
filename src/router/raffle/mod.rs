pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};

use path::RafflePath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(RafflePath::GetEligible.as_str(), get(handler::get_eligible))
        .route(RafflePath::Check.as_str(), get(handler::check_raffle))
        .route(RafflePath::Round.as_str(), get(handler::get_round))
}
