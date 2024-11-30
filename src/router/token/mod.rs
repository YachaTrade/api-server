pub mod path;

pub mod handler;
use axum::{
    middleware,
    routing::{get, put},
    Router,
};
use handler::{get_token, update_token};
use path::TokenPath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(TokenPath::GetToken.as_str(), get(get_token))
        .route(
            TokenPath::UpdateToken.as_str(),
            put(update_token).layer(middleware::from_fn_with_state(state, authenticate_user)),
        )
}
