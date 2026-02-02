pub mod handler;
pub mod path;

use axum::{
    Router,
    middleware as axum_middleware,
    routing::{delete, get},
};
use tower::ServiceBuilder;

use path::ApiKeyPath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    let auth_layer = ServiceBuilder::new().layer(axum_middleware::from_fn_with_state(
        app_state,
        authenticate_user,
    ));

    Router::new()
        .route(
            ApiKeyPath::ApiKey.as_str(),
            get(handler::list_api_keys_handler)
                .post(handler::create_api_key_handler)
                .layer(auth_layer.clone()),
        )
        .route(
            ApiKeyPath::ApiKeyId.as_str(),
            delete(handler::revoke_api_key_handler).layer(auth_layer),
        )
}
