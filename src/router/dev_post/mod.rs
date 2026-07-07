pub mod handler;
pub mod path;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware as axum_middleware,
    routing::{get, patch, post},
};

use path::DevPostPath;

use crate::{middleware::authenticate_user, state::AppState};

/// Public GETs and protected writes share paths (`/dev-post` GET+POST,
/// `/dev-post/{post_id}` GET+PATCH+DELETE), so they're built as two routers
/// and merged — `route_layer` then applies auth only to the protected half.
pub fn router(app_state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route(DevPostPath::Trending.as_str(), get(handler::get_trending))
        .route(DevPostPath::Ranking.as_str(), get(handler::get_ranking))
        .route(DevPostPath::Feed.as_str(), get(handler::get_feed))
        .route(DevPostPath::Detail.as_str(), get(handler::get_detail));

    let protected = Router::new()
        .route(
            DevPostPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for image upload
        )
        .route(DevPostPath::Create.as_str(), post(handler::create_post))
        .route(
            DevPostPath::Detail.as_str(),
            patch(handler::edit_post).delete(handler::delete_post),
        )
        .route(
            DevPostPath::Like.as_str(),
            post(handler::like).delete(handler::unlike),
        )
        .route(DevPostPath::Vote.as_str(), post(handler::vote))
        .route_layer(axum_middleware::from_fn_with_state(
            app_state,
            authenticate_user,
        ));

    public.merge(protected)
}
