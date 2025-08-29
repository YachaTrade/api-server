pub mod handler;
pub mod path;
use axum::{
    Router,
    routing::{get, patch, put},
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
        .route(AccountPath::ConnectX.as_str(), put(handler::connect_x))
        // .route(
        //     AccountPath::DisconnectX.as_str(),
        //     delete(handler::disconnect_x),
        // )
        .route(AccountPath::GetX.as_str(), get(handler::get_x_handle))
        .route(
            AccountPath::RegisterWallet.as_str(),
            patch(handler::register_wallet),
        )
        .route(AccountPath::GetWallet.as_str(), get(handler::get_wallet))
}
