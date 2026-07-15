pub mod analytics;
pub mod handler;
pub mod path;

use crate::{middleware::authenticate_user, state::AppState};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware::from_fn_with_state,
    routing::{delete, post},
};
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
                .layer(DefaultBodyLimit::max(5_000_000)) // 5MB for image upload
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::UploadDexTokenImage.as_str(),
            post(handler::upload_dex_token_image)
                .layer(DefaultBodyLimit::max(5_000_000)) // 5MB for image upload
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::UpsertWhitelistToken.as_str(),
            post(handler::upsert_whitelist_token)
                .get(handler::list_whitelist_token)
                .layer(DefaultBodyLimit::max(5_000_000)) // 5MB for image upload
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::DeleteDevPost.as_str(),
            delete(handler::delete_dev_post)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::RestoreDevPost.as_str(),
            post(handler::restore_dev_post)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .merge(analytics::router(state.clone()))
}
