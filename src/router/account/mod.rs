pub mod handler;
pub mod path;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
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
        // .route(AccountPath::ConnectX.as_str(), put(handler::connect_x))
        // .route(
        //     AccountPath::DisconnectX.as_str(),
        //     delete(handler::disconnect_x),
        // )
        .route(AccountPath::UpdateX.as_str(), patch(handler::update_x))
        .route(
            AccountPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for image upload
        )
        .route(
            AccountPath::RegisterWallet.as_str(),
            patch(handler::register_wallet),
        )
        .route(AccountPath::GetWallet.as_str(), get(handler::get_wallet))
}
