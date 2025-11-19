pub mod handler;
pub mod path;

use crate::{middleware::authenticate_user, state::AppState};
use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};
use path::TrendPath;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(TrendPath::GetTrend.as_str(), get(handler::get_trend))
        .route(
            TrendPath::InsertTrend.as_str(),
            post(handler::insert_trend).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            TrendPath::DeleteTrend.as_str(),
            delete(handler::delete_trend).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
}
