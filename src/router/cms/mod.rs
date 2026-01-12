pub mod handler;
pub mod path;

use crate::{middleware::authenticate_user, state::AppState};
use axum::{Router, middleware::from_fn_with_state, routing::post};
use path::CmsPath;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            CmsPath::SetNsfw.as_str(),
            post(handler::set_nsfw).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::InsertTrend.as_str(),
            post(handler::insert_trend).layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::UpdateMetadata.as_str(),
            post(handler::update_metadata)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
}
