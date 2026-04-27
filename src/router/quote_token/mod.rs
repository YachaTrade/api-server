pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::QuoteTokenPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(QuoteTokenPath::List.as_str(), get(handler::list_quote_tokens))
}
