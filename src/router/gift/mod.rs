//! Public X webhook routes (CRC + Account Activity events).
mod handler;
pub mod path;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/x/webhook",
            get(handler::crc_handler).post(handler::event_handler),
        )
        .route("/x/healthz", get(handler::healthz_handler))
}
