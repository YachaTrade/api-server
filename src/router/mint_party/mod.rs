use axum::{
    routing::{get, put},
    Router,
};
use path::MintPartyPath;

use crate::state::AppState;

pub mod handler;
pub mod path;

pub fn router() -> Router<AppState> {
    Router::new().route(
        MintPartyPath::UpdateMintParty.as_str(),
        put(handler::update_mint_party),
    )
}
