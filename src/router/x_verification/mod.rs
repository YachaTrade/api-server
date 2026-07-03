pub mod handler;
pub mod path;

use axum::{
    Router,
    routing::{delete, get, post},
};
use tower::ServiceBuilder;

use axum::middleware as axum_middleware;

use crate::middleware::authenticate_user;
use crate::state::AppState;
use path::XVerificationPath;

pub fn router(app_state: AppState) -> Router<AppState> {
    let auth_layer = ServiceBuilder::new().layer(axum_middleware::from_fn_with_state(
        app_state,
        authenticate_user,
    ));

    Router::new()
        // Public: X redirects the browser here (no session/Origin).
        .route(
            XVerificationPath::OauthCallback.as_str(),
            get(handler::oauth_callback),
        )
        // Session-required routes.
        .route(
            XVerificationPath::OauthLogin.as_str(),
            post(handler::oauth_login).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::FollowedBy.as_str(),
            post(handler::add_followed_by).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::FollowedByDelete.as_str(),
            delete(handler::delete_followed_by).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::Pending.as_str(),
            get(handler::get_pending).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::Reserve.as_str(),
            post(handler::reserve).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::Finalize.as_str(),
            post(handler::finalize).layer(auth_layer),
        )
}
