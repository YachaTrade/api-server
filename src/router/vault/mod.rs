pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::VaultPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        VaultPath::GetTokenVaults.as_str(),
        get(handler::get_token_vaults),
    )
}
