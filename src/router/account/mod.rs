pub mod handler;
pub mod path;
use axum::{
    routing::{get, patch},
    Router,
};

use path::AccountPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            AccountPath::UpdateAccount.as_str(),
            patch(handler::update_account),
        )
        .route(AccountPath::GetAccount.as_str(), get(handler::get_account))
}
