pub mod path;

pub mod handler;
use axum::{routing::get, Router};
use handler::get_balance;
use path::Path;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(Path::GetBalance.as_str(), get(get_balance))
}
