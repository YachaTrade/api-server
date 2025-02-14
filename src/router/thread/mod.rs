pub mod handler;
pub mod path;

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};

use handler::{
    create_thread, get_thread_like_by_account, get_threads_by_token, like_thread, unlike_thread,
};
use path::Path;

use crate::middleware::authenticate_user;
use crate::state::AppState;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            Path::CreateThread.as_str(),
            post(create_thread).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            Path::LikeThread.as_str(),
            post(like_thread).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            Path::UnLikeThread.as_str(),
            delete(unlike_thread).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            Path::GetThreadLike.as_str(),
            get(get_thread_like_by_account)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
        )
        .route(Path::GetThread.as_str(), get(get_threads_by_token))
}
