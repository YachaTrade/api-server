pub mod handler;
pub mod path;

use axum::{Router, routing::post};
pub use path::MetadataPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            MetadataPath::UploadImage.as_str(),
            post(handler::upload_image),
        )
        .route(
            MetadataPath::UploadMetadata.as_str(),
            post(handler::upload_metadata),
        )
}
