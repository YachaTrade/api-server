use axum::{
    middleware,
    routing::{get, put},
    Router,
};
use path::MintPartyPath;

use crate::{middleware::authenticate_user, state::AppState};

pub mod handler;
pub mod path;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new().route(
        MintPartyPath::UpdateMintParty.as_str(),
        put(handler::update_mint_party)
            .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
    )
}
