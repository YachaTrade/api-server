pub mod handler;
pub mod path;

use crate::{middleware::authenticate_user, state::AppState};
use axum::{Router, middleware::from_fn_with_state, routing::get};
use path::AnalyticsPath;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            AnalyticsPath::ChurnedUsers.as_str(),
            get(handler::get_churned_users)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            AnalyticsPath::ActiveUsers.as_str(),
            get(handler::get_active_users)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            AnalyticsPath::NewUsers.as_str(),
            get(handler::get_new_users).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            AnalyticsPath::UserRoi.as_str(),
            get(handler::get_user_roi).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            AnalyticsPath::ChesterRetention.as_str(),
            get(handler::get_chester_retention)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            AnalyticsPath::CreatorFee.as_str(),
            get(handler::get_creator_fee)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
}
