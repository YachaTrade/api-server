pub mod handler;
pub mod path;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, post},
    Router,
};
use handler::{auth_delete_session, auth_nonce, auth_session};
use path::Path;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(Path::Nonce.as_str(), post(auth_nonce))
        .route(Path::Session.as_str(), post(auth_session))
        .route(
            Path::DeleteSession.as_str(),
            delete(auth_delete_session)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
        )
}
