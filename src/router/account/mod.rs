pub mod handler;
pub mod path;
use axum::{
    routing::{get, patch},
    Router,
};

use handler::{add_account_like, get_account, remove_account_like, update_account};
use path::Path;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(Path::UpdateAccount.as_str(), patch(update_account))
        .route(Path::GetAccount.as_str(), get(get_account))
        .route(Path::AddAccountLike.as_str(), patch(add_account_like))
        .route(Path::RemoveAccountLike.as_str(), patch(remove_account_like))
}
