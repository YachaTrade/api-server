pub mod handler;
pub mod path;

use axum::{Router, routing::post};

use handler::set_metadata;
use path::BotPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(&BotPath::SetMetadata.as_str(), post(set_metadata))
}
