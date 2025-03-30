use axum::extract::{Multipart, State};
use axum::response::IntoResponse;
use axum::Json;

use serde_json::json;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::result::AppResult;
use crate::state::AppState;
use crate::types::bot::{
    parse_multipart_data, parse_upload_response, request_update_metadata_url,
    request_upload_image_url, upload_image_to_url, upload_metadata_to_url, validate_content_type,
    BotMetadataResponse,
};

#[instrument(skip(_state), fields(multipart))]
pub async fn set_metadata(
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    // Parse multipart form data
    let (mut metadata, image_file) = parse_multipart_data(&mut multipart).await?;
    let (image_data, image_content_type) = image_file;
    validate_content_type(&image_content_type);
    info!("Metadata parse success {:#?}", metadata);
    // Generate unique ID for file names

    let unique_id = Uuid::new_v4().to_string();
    let image_file_name = format!("coin/{}", unique_id);

    let image_upload_request_response =
        request_upload_image_url(&image_file_name, &image_content_type, image_data.len()).await?;

    let (upload_url, image_url) = parse_upload_response(&image_upload_request_response)?;
    // Upload image and get image URL
    info!("image url {}", image_url);
    metadata.image_uri = Some(image_url);
    upload_image_to_url(
        &upload_url,
        &image_data,
        &image_content_type,
        &image_file_name,
    )
    .await?;

    let metadata_file_name = format!("metadata-{}.json", unique_id);

    let metadata_upload_response =
        request_update_metadata_url(&metadata_file_name, &metadata).await?;

    let (upload_url, metadata_url) = parse_upload_response(&metadata_upload_response)?;

    upload_metadata_to_url(&upload_url, metadata, &metadata_file_name).await?;

    Ok(Json(BotMetadataResponse { metadata_url }))
}
