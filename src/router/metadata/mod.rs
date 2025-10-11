pub mod handler;
pub mod path;

use axum::{Router, extract::DefaultBodyLimit, routing::{get, post}};
pub use path::MetadataPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            MetadataPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route(
            MetadataPath::UploadMetadata.as_str(),
            post(handler::upload_metadata),
        )
        .route(
            MetadataPath::GetGeckoMetadata.as_str(),
            get(handler::get_gecko_metadata),
        )
}
