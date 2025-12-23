pub mod handler;
pub mod path;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware as axum_middleware,
    routing::{get, patch, post},
};
use tower::ServiceBuilder;

use path::AccountPath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    let auth_layer = ServiceBuilder::new().layer(axum_middleware::from_fn_with_state(
        app_state,
        authenticate_user,
    ));

    Router::new()
        // Public routes (no auth)
        .route(
            AccountPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for image upload
        )
        // Protected routes (auth required)
        .route(
            AccountPath::UpdateAccount.as_str(),
            patch(handler::update_account).layer(auth_layer.clone()),
        )
        .route(
            AccountPath::GetAccount.as_str(),
            get(handler::get_account).layer(auth_layer.clone()),
        )
        .route(
            AccountPath::UpdateX.as_str(),
            patch(handler::update_x).layer(auth_layer.clone()),
        )
        .route(
            AccountPath::RegisterWallet.as_str(),
            patch(handler::register_wallet).layer(auth_layer.clone()),
        )
        .route(
            AccountPath::GetWallet.as_str(),
            get(handler::get_wallet).layer(auth_layer),
        )
}
