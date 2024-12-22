pub mod path;

pub mod handler;
use axum::{routing::get, Router};
use handler::get_token;
use path::TokenPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(TokenPath::GetToken.as_str(), get(get_token))
}
