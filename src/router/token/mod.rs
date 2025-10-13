pub mod path;

pub mod handler;
use axum::{
    Router,
    routing::{get, post},
};

use path::TokenPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(TokenPath::GetToken.as_str(), get(handler::get_token))
        .route(
            TokenPath::GetMetadata.as_str(),
            get(handler::get_token_metadata),
        )
        .route(TokenPath::Salt.as_str(), post(handler::salt))
}
