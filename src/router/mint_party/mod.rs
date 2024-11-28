use axum::{
    routing::{get, put},
    Router,
};

use crate::state::AppState;

pub mod handler;
pub mod path;

pub fn router() -> Router<AppState> {
    Router::new().route(
        path::ProfilePath::UpdateMintParty.as_str(),
        put(handler::update_mint_party),
    )
}
