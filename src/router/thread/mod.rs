pub mod handler;
pub mod path;

use axum::{
    routing::{delete, post, put},
    Router,
};

use handler::{create_thread, like_thread, unlike_thread};
use path::Path;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(Path::CreateThread.as_str(), post(create_thread))
        // .route(Path::FixThread.as_str(), put(fix_thread))
        // .route(Path::RemoveThread.as_str(), delete(delete_thread))
        .route(Path::LikeThread.as_str(), post(like_thread))
        .route(Path::UnLikeThread.as_str(), delete(unlike_thread))
}
