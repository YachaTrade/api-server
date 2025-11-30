pub mod handler;
pub mod path;

use axum::{Router, extract::DefaultBodyLimit, routing::post};
pub use path::MetadataPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            MetadataPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for image upload
        )
        .route(
            MetadataPath::UploadMetadata.as_str(),
            post(handler::upload_metadata).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for metadata upload
        )
}
