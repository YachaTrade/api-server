use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use path::{ActiveUserPath, PointPath};

use crate::{middleware::authenticate_user, state::AppState};

pub mod handler;
pub mod path;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            ActiveUserPath::CheckActiveUser.as_str(),
            get(handler::check_active_user),
        )
        .route(PointPath::GetTop.as_str(), get(handler::get_top_point))
        .route(
            PointPath::GetAccountPoint.as_str(),
            get(handler::get_point_by_account_id).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            PointPath::CompleteMission.as_str(),
            post(handler::complete_mission).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            PointPath::GetCompletedMissions.as_str(),
            get(handler::get_completed_missions)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
        )
}
