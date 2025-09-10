use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::Json,
};
use bytes::Bytes;
use std::time::Instant;
use tracing::info;
use utoipa;
use uuid::Uuid;

use crate::{
    result::{AppError, AppJsonResult},
    router::metadata::MetadataPath,
    state::AppState,
    types::metadata::{
        MetadataController, TokenMetadata, UploadImageMultipart, UploadImageResponse, UploadMetadataRequest,
        UploadMetadataResponse,
    },
};
use aws_config::BehaviorVersion;
use aws_sdk_rekognition::Client;
use aws_sdk_rekognition::primitives::Blob;
use aws_sdk_rekognition::types::Image;


// 허용된 이미지 타입 상수 정의
const ALLOWED_IMAGE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/svg+xml"];

/// Validate image with both MIME type and actual format
fn validate_image(data: &[u8], content_type: &Option<String>) -> Result<String, AppError> {
    // First check if declared content type is allowed
    if let Some(ct) = content_type {
        if !ALLOWED_IMAGE_TYPES.contains(&ct.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Unsupported image type: {}",
                ct
            )));
        }
    } else {
        return Err(AppError::BadRequest("Missing content type".to_string()));
    }

    // Check actual file format by magic bytes
    if data.len() < 4 {
        return Err(AppError::BadRequest("File too small".to_string()));
    }

    let actual_format = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // JPEG: FF D8 FF
        "image/jpeg"
    } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        "image/png"
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        // WebP: RIFF....WEBP
        "image/webp"
    } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
        // SVG: starts with < or <?xml
        if let Ok(content) = std::str::from_utf8(data) {
            if content.contains("<svg") {
                "image/svg+xml"
            } else {
                return Err(AppError::BadRequest("Invalid SVG format".to_string()));
            }
        } else {
            return Err(AppError::BadRequest("Invalid SVG encoding".to_string()));
        }
    } else {
        return Err(AppError::BadRequest("Invalid image format".to_string()));
    };

    // Check if declared type matches actual format
    if let Some(declared_type) = content_type {
        if declared_type != actual_format {
            return Err(AppError::BadRequest(format!(
                "File format mismatch: declared {} but actual {}",
                declared_type, actual_format
            )));
        }
    }

    Ok(actual_format.to_string())
}

/// Check if image is NSFW using AWS Rekognition
async fn check_nsfw(image_data: &[u8]) -> Result<bool, AppError> {
    // AWS 설정 로드 (ap-northeast-1: 도쿄 리전)
    // AWS SDK가 자동으로 환경 변수에서 인증 정보를 찾습니다
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region("ap-northeast-1")
        .load()
        .await;

    let client = Client::new(&config);

    // Bytes를 Blob으로 변환
    let bytes = Bytes::copy_from_slice(image_data);
    let blob = Blob::new(bytes);
    let image = Image::builder().bytes(blob).build();

    // Content Moderation API 호출
    let resp = client
        .detect_moderation_labels()
        .image(image)
        .min_confidence(10.0)
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("AWS Rekognition error: {}", e)))?;

    // 성인물 여부 판단
    let labels = resp.moderation_labels();
    let is_nsfw = is_adult_content(labels);

    Ok(is_nsfw)
}

/// 성인물 여부를 판단하는 함수
fn is_adult_content(labels: &[aws_sdk_rekognition::types::ModerationLabel]) -> bool {
    // AWS 라벨 기반 NSFW 차단 기준
    let adult_categories = [
        ("Explicit", 10.0),                 // 명시적 콘텐츠
        ("Explicit Nudity", 10.0),          // 명시적 누드
        ("Explicit Sexual Activity", 10.0), // 명시적 성행위
        ("Exposed Buttocks or Anus", 10.0), // 항문/엉덩이 노출
        ("Exposed Male Genitalia", 10.0),   // 남성 성기 노출
        ("Exposed Female Genitalia", 10.0), // 여성 성기 노출
        ("Exposed Female Nipple", 10.0),    // 여성 젖꼭지 노출
        ("Non-Explicit Nudity", 90.0),      // 비명시적 누드 (높은 임계값)
    ];

    // 성인물 검사
    for label in labels {
        let name = label.name().unwrap_or_default();
        let confidence = label.confidence().unwrap_or_default();

        // 카테고리 이름 정확히 일치 여부 확인
        for (category, threshold) in &adult_categories {
            if name.to_lowercase() == category.to_lowercase() && confidence >= *threshold {
                return true;
            }
        }
    }

    false
}

/// Extract image from multipart form data
async fn extract_image_from_multipart(
    mut multipart: Multipart,
) -> Result<(Bytes, Option<String>), StatusCode> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if let Some(name) = field.name() {
            if name == "image" {
                let content_type = field.content_type().map(|ct| ct.to_string());
                let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                return Ok((data, content_type));
            }
        }
    }
    Err(StatusCode::BAD_REQUEST)
}

/// Upload image with NSFW validation
#[utoipa::path(
    post,
    path = MetadataPath::UploadImage.docs_str(),
    request_body(
        content = UploadImageMultipart,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "Image uploaded successfully", body = UploadImageResponse),
        (status = 400, description = "Bad request - Invalid image format or missing image"),
        (status = 500, description = "Internal server error - NSFW check failed or upload failed")
    ),
    tag = "Metadata"
)]
pub async fn upload_image(
    State(state): State<AppState>,
    multipart: Multipart,
) -> AppJsonResult<UploadImageResponse> {
    let start_time = Instant::now();
    info!("🚀 Starting image upload process");

    let (image_data, content_type) = extract_image_from_multipart(multipart)
        .await
        .map_err(|_| AppError::BadRequest("No image found in image key".to_string()))?;

    let image_id = Uuid::new_v4().to_string();

    let validated_format = validate_image(&image_data, &content_type)?;

    let (upload_result, nsfw_result) = tokio::join!(
        state
            .r2
            .upload_metadata_image_file(&image_id, &image_data, &validated_format),
        check_nsfw(&image_data),
    );

    let image_url = upload_result?;
    let is_nsfw = nsfw_result?;

    // Cache NSFW result in Redis
    let cache_start = Instant::now();
    if let Err(e) = state.redis.set_nsfw_status(&image_url, is_nsfw).await {
        tracing::error!("Failed to cache NSFW status: {}", e);
    }
    info!("💾 Redis caching took: {:?}", cache_start.elapsed());

    let total_duration = start_time.elapsed();
    info!(
        "🎉 Image upload completed - Total time: {:?}, Image URL: {}, NSFW: {}",
        total_duration, image_url, is_nsfw
    );

    Ok(Json(UploadImageResponse { is_nsfw, image_url }))
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

    // Check NSFW status from cache (required)
    let is_nsfw = match state.redis.get_nsfw_status(&payload.image_url).await {
        Ok(is_nsfw) => is_nsfw,
        Err(_) => {
            return Err(AppError::BadRequest(
                "NSFW status unknown for this image".to_string(),
            ));
        }
    };

    let metadata = TokenMetadata {
        name: payload.name,
        symbol: payload.symbol,
        description: payload.description,
        image_url: payload.image_url,
        website: payload.website,
        twitter: payload.twitter,
        telegram: payload.telegram,
        is_nsfw,
    };

    // Validate metadata
    metadata.validate()?;

    // Generate unique metadata ID
    let metadata_id = Uuid::new_v4().to_string();

    // Upload metadata to R2
    let metadata_url = state
        .r2
        .upload_metadata_file(&metadata_id, &metadata)
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to upload metadata: {}", e)))?;

    info!("📤 Metadata uploaded to R2: {}", metadata_url);

    // Save to database
    let db_start = Instant::now();
    let metadata_controller = MetadataController::new(state.postgres.clone());
    metadata_controller
        .save_token_metadata(&metadata, &metadata_url)
        .await
        .map_err(|e| {
            AppError::InternalError(format!("Failed to save metadata to database: {}", e))
        })?;
    info!("💾 Database save took: {:?}", db_start.elapsed());

    let total_duration = start_time.elapsed();
    info!(
        "🎉 Metadata upload completed - Total time: {:?}, Metadata URL: {}",
        total_duration, metadata_url
    );

    Ok(Json(UploadMetadataResponse {
        metadata_url,
        metadata,
    }))
}
