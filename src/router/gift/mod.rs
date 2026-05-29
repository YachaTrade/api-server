//! Public X webhook routes (CRC + Account Activity events).
mod handler;
pub mod path;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/gift/webhook",
            get(handler::crc_handler).post(handler::event_handler),
        )
        .route("/gift/healthz", get(handler::healthz_handler))
}
