use axum::{extract::State, http::HeaderMap, response::Json};
use bytes::Bytes;
use std::time::Instant;
use tracing::info;
use utoipa;

use crate::{
    result::AppJsonResult,
    router::metadata::MetadataPath,
    services::metadata::MetadataService,
    state::AppState,
    types::metadata::{UploadImageResponse, UploadMetadataRequest, UploadMetadataResponse},
};

/// Upload image with NSFW validation
#[utoipa::path(
    post,
    path = MetadataPath::UploadImage.docs_str(),
    request_body(
        content = Vec<u8>,
        description = "Raw image binary data (supported formats: image/jpeg, image/png, image/webp, image/svg+xml)",
        content_type = "image/png"
    ),
    responses(
        (status = 200, description = "Image uploaded successfully", body = UploadImageResponse),
        (status = 400, description = "Bad request - Invalid image format or missing image"),
        (status = 413, description = "Payload too large - Image exceeds 5MB limit"),
        (status = 500, description = "Internal server error - NSFW check failed or upload failed")
    ),
    tag = "Metadata"
)]
pub async fn upload_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppJsonResult<UploadImageResponse> {
    let start_time = Instant::now();
    info!("🚀 Starting image upload process");
    info!("📋 Request Headers: {:?}", headers);
    info!("📦 Reading binary body - Size: {} bytes", body.len());

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );

    let response = service
        .process_and_upload_image(&body, &content_type)
        .await?;

    let total_duration = start_time.elapsed();
    info!(
        "🎉 Image upload completed - Total time: {:?}, Image URI: {}, NSFW: {}",
        total_duration, response.image_uri, response.is_nsfw
    );

    Ok(Json(response))
}

/// Upload metadata to R2 and DB
#[utoipa::path(
    post,
    path = MetadataPath::UploadMetadata.docs_str(),
    request_body = UploadMetadataRequest,
    responses(
        (status = 200, description = "Metadata uploaded successfully", body = UploadMetadataResponse),
        (status = 400, description = "Bad request - NSFW status unknown for image or invalid data"),
        (status = 500, description = "Internal server error - Upload to R2 or database failed")
    ),
    tag = "Metadata"
)]
pub async fn upload_metadata(
    State(state): State<AppState>,
    Json(payload): Json<UploadMetadataRequest>,
) -> AppJsonResult<UploadMetadataResponse> {
    let start_time = Instant::now();
    info!("🚀 Starting metadata upload process for: {}", payload.name);

    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );
    let metadata = service.validate_metadata_request(&payload).await?;
    let response = service.upload_metadata(metadata).await?;

    let total_duration = start_time.elapsed();
    info!(
        "🎉 Metadata upload completed - Total time: {:?}, Metadata URI: {}",
        total_duration, response.metadata_uri
    );

    Ok(Json(response))
}
