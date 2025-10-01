use axum::{
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use bytes::Bytes;
use image::{GenericImageView, ImageFormat, imageops::FilterType};
use std::{env, io::Cursor, time::Instant};
use tracing::info;
use utoipa;
use uuid::Uuid;

use crate::{
    result::{AppError, AppJsonResult},
    router::metadata::MetadataPath,
    services::metadata::MetadataService,
    state::AppState,
    types::metadata::{UploadImageResponse, UploadMetadataRequest, UploadMetadataResponse},
};
use aws_config::{BehaviorVersion, Region};
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

/// Convert any image format to PNG for validation with optional resize
fn convert_to_png(image_data: &[u8], max_width: u32, max_height: u32) -> Result<Vec<u8>, AppError> {
    info!("🚀 Starting image conversion process");
    let start_time = Instant::now();

    let img = image::load_from_memory(image_data)
        .map_err(|e| AppError::BadRequest(format!("Failed to decode image: {}", e)))?;

    // Resize if larger than max dimensions
    let (width, height) = img.dimensions();
    info!("📐 Original image size: {}x{}", width, height);

    let processed_img = if width > max_width || height > max_height {
        info!("🔄 Resizing image to fit {}x{}", max_width, max_height);
        img.resize(max_width, max_height, FilterType::Lanczos3)
    } else {
        img
    };

    let mut png_data = Vec::new();
    let mut cursor = Cursor::new(&mut png_data);

    processed_img
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| AppError::InternalError(format!("Failed to convert to PNG: {}", e)))?;

    let conversion_time = start_time.elapsed();
    info!(
        "✅ PNG conversion completed - Time: {:?}, Output size: {} bytes",
        conversion_time,
        png_data.len()
    );

    Ok(png_data)
}

/// Check if image is NSFW using AWS Rekognition (uses PNG converted data)
async fn check_nsfw(image_data: &[u8]) -> Result<bool, AppError> {
    info!(
        "🔍 Starting NSFW check - Image size: {} bytes",
        image_data.len()
    );

    // Convert to PNG and resize for Rekognition (max 512x512)
    // Run conversion in blocking thread pool to avoid blocking async runtime
    let start_conversion = Instant::now();
    let image_data_owned = image_data.to_vec();
    let png_data =
        tokio::task::spawn_blocking(move || convert_to_png(&image_data_owned, 1024, 1024))
            .await
            .map_err(|e| AppError::InternalError(format!("Task join error: {}", e)))??;

    info!(
        "⏱️  Image conversion took: {:?}",
        start_conversion.elapsed()
    );

    // AWS 설정 로드 - 환경 변수에서 리전 가져오기
    // AWS SDK가 자동으로 환경 변수에서 인증 정보를 찾습니다
    info!("☁️  Loading AWS configuration");
    let aws_region = env::var("AWS_REGION").expect("AWS_REGION must be set");
    let region = Region::new(aws_region);
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(region)
        .load()
        .await;

    info!("📡 Creating Rekognition client");
    let client = Client::new(&config);

    // PNG 데이터를 Blob으로 변환
    let bytes = Bytes::from(png_data);
    let blob = Blob::new(bytes);
    let image = Image::builder().bytes(blob).build();

    // Content Moderation API 호출
    info!("🌐 Calling AWS Rekognition API");
    let api_start = Instant::now();
    let resp = client
        .detect_moderation_labels()
        .image(image)
        .min_confidence(10.0)
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("AWS Rekognition error: {}", e)))?;

    info!("⏱️  Rekognition API took: {:?}", api_start.elapsed());

    // 성인물 여부 판단
    let labels = resp.moderation_labels();
    let is_nsfw = is_adult_content(labels);

    info!("✅ NSFW check completed - Result: {}", is_nsfw);

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
    info!("🔄 Starting multipart field iteration");
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if let Some(name) = field.name() {
            info!("📋 Found field: {}", name);
            if name == "image" {
                let content_type = field.content_type().map(|ct| ct.to_string());
                info!("📥 Reading image bytes from field");
                let start = Instant::now();
                let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                info!("✅ Read {} bytes in {:?}", data.len(), start.elapsed());
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
        content = crate::types::metadata::UploadImageMultipart,
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
    headers: HeaderMap,
    multipart: Multipart,
) -> AppJsonResult<UploadImageResponse> {
    let start_time = Instant::now();
    info!("🚀 Starting image upload process");
    info!("📋 Request Headers: {:?}", headers);

    info!("📦 Extracting image from multipart");
    let (image_data, content_type) = extract_image_from_multipart(multipart)
        .await
        .map_err(|_| AppError::BadRequest("No image found in image key".to_string()))?;

    info!("✅ Image extracted - Size: {} bytes", image_data.len());

    let image_id = Uuid::new_v4().to_string();

    info!("🔍 Validating image format");
    let validated_format = validate_image(&image_data, &content_type)?;
    info!("✅ Image format validated: {}", validated_format);

    let is_nsfw = check_nsfw(&image_data).await?;

    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );
    let response = service
        .upload_image(&image_id, &image_data, &validated_format, is_nsfw)
        .await?;

    let total_duration = start_time.elapsed();
    info!(
        "🎉 Image upload completed - Total time: {:?}, Image URL: {}, NSFW: {}",
        total_duration, response.image_url, response.is_nsfw
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
        "🎉 Metadata upload completed - Total time: {:?}, Metadata URL: {}",
        total_duration, response.metadata_url
    );

    Ok(Json(response))
}
