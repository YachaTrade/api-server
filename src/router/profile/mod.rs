pub mod handler;
pub mod path;
use crate::{middleware::authenticate_user, state::AppState};

use axum::{Router, middleware, routing::get};

use path::ProfilePath;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(ProfilePath::GetProfile.as_str(), get(handler::get_profile))
        .route(
            ProfilePath::GetHoldToken.as_str(),
            get(handler::get_hold_token),
        )
        .route(
            ProfilePath::GetTokenCreated.as_str(),
            get(handler::get_token_created),
        )
        .route(
            ProfilePath::GetSwapHistory.as_str(),
            get(handler::get_swap_history),
        )
        .route(
            ProfilePath::GetPointHistory.as_str(),
            get(handler::get_point_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
}
