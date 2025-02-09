pub mod handler;
pub mod path;

use axum::{
    routing::{delete, get, post},
    Router,
};

use handler::{
    create_thread, get_thread_like_by_account, get_threads_by_token, like_thread, unlike_thread,
};
use path::Path;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(Path::CreateThread.as_str(), post(create_thread))
        .route(Path::GetThread.as_str(), get(get_threads_by_token))
        .route(Path::LikeThread.as_str(), post(like_thread))
        .route(Path::UnLikeThread.as_str(), delete(unlike_thread))
        .route(
            Path::GetThreadLike.as_str(),
            get(get_thread_like_by_account),
        )
}
