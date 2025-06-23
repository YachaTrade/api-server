pub mod handler;
pub mod path;
use axum::{routing::get, Router};

use path::ManagementPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            ManagementPath::DevPosition.as_str(),
            get(handler::get_dev_positions),
        )
        .route(
            ManagementPath::HoldingTokenManagements.as_str(),
            get(handler::get_holding_token_management),
        )
        .route(
            ManagementPath::AccountLocks.as_str(),
            get(handler::get_account_locks),
        )
        .route(
            ManagementPath::AccountWithdrawableLock.as_str(),
            get(handler::get_account_withdrawable_lock),
        )
}
