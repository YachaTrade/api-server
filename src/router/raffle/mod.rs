pub mod handler;
pub mod path;

use axum::{Router, middleware as axum_middleware, routing::get};
use tower::ServiceBuilder;

use crate::{middleware::authenticate_user, state::AppState};

use path::RafflePath;

pub fn router(app_state: AppState) -> Router<AppState> {
    let auth_layer = ServiceBuilder::new()
        .layer(axum_middleware::from_fn_with_state(app_state, authenticate_user));

    Router::new()
        .route(
            RafflePath::GetEligible.as_str(),
            get(handler::get_eligible).layer(auth_layer.clone()),
        )
        .route(
            RafflePath::Check.as_str(),
            get(handler::check_raffle).layer(auth_layer),
        )
        .route(RafflePath::Round.as_str(), get(handler::get_round))
}
