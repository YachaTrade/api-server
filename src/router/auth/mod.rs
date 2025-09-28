pub mod handler;
pub mod path;

use axum::{
    Router, middleware,
    routing::{delete, post},
};
use handler::{auth_delete_session, auth_nonce, auth_session};
use path::AuthPath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(AuthPath::Nonce.as_str(), post(auth_nonce))
        .route(AuthPath::Session.as_str(), post(auth_session))
        .route(
            AuthPath::DeleteSession.as_str(),
            delete(auth_delete_session)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
        )
}
