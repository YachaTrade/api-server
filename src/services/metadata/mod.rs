use bytes::Bytes;
use std::{sync::Arc, io::Cursor, time::Instant, env};
use uuid::Uuid;
use image::{GenericImageView, ImageFormat, imageops::FilterType};
use tracing::info;
use resvg::usvg;
use tiny_skia::Pixmap;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_rekognition::Client;
use aws_sdk_rekognition::primitives::Blob;
use aws_sdk_rekognition::types::Image;

use crate::{
    controllers::metadata::MetadataController,
    db::{R2::R2Client, postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::metadata::{
        GeckoMetadataResponse, TokenMetadata, UploadImageResponse, UploadMetadataRequest, UploadMetadataResponse,
    },
};

// 허용된 이미지 타입 상수 정의
const ALLOWED_IMAGE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/svg+xml"];

pub struct MetadataService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
    r2: Arc<R2Client>,
}

impl MetadataService {
    pub fn new(
        postgres: Arc<PostgresDatabase>,
        redis: Arc<RedisDatabase>,
        r2: Arc<R2Client>,
    ) -> Self {
        Self {
            postgres,
            redis,
            r2,
        }
    }

    /// Validate image with both MIME type and actual format
    fn validate_image(&self, data: &[u8], content_type: &Option<String>) -> Result<String, AppError> {
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
            "image/jpeg"
        } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            "image/png"
        } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            "image/webp"
        } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
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

    /// Convert SVG to PNG
    fn convert_svg_to_png(&self, svg_data: &[u8], max_width: u32, max_height: u32) -> Result<Vec<u8>, AppError> {
        info!("🎨 Starting SVG to PNG conversion");
        let start_time = Instant::now();

        let opts = usvg::Options::default();
        let tree = usvg::Tree::from_data(svg_data, &opts)
            .map_err(|e| AppError::BadRequest(format!("Failed to parse SVG: {}", e)))?;

        let svg_size = tree.size();
        info!("📐 SVG size: {}x{}", svg_size.width(), svg_size.height());

        let scale = (max_width as f32 / svg_size.width()).min(max_height as f32 / svg_size.height()).min(1.0);
        let target_width = (svg_size.width() * scale) as u32;
        let target_height = (svg_size.height() * scale) as u32;

        info!("🔄 Rendering SVG to {}x{}", target_width, target_height);

        let mut pixmap = Pixmap::new(target_width, target_height)
            .ok_or_else(|| AppError::InternalError("Failed to create pixmap".to_string()))?;

        let tree_size = tree.size().to_int_size();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                target_width as f32 / tree_size.width() as f32,
                target_height as f32 / tree_size.height() as f32,
            ),
            &mut pixmap.as_mut(),
        );

        let png_data = pixmap.encode_png()
            .map_err(|e| AppError::InternalError(format!("Failed to encode PNG: {}", e)))?;

        let conversion_time = start_time.elapsed();
        info!(
            "✅ SVG to PNG conversion completed - Time: {:?}, Output size: {} bytes",
            conversion_time,
            png_data.len()
        );

        Ok(png_data)
    }

    /// Convert any image format to PNG for validation with optional resize
    fn convert_to_png(&self, image_data: &[u8], max_width: u32, max_height: u32) -> Result<Vec<u8>, AppError> {
        info!("🚀 Starting image conversion process");
        let start_time = Instant::now();

        let img = image::load_from_memory(image_data)
            .map_err(|e| AppError::BadRequest(format!("Failed to decode image: {}", e)))?;

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

    /// Check if image is NSFW using AWS Rekognition
    async fn check_nsfw(&self, image_data: &[u8], format: &str) -> Result<bool, AppError> {
        info!(
            "🔍 Starting NSFW check - Image size: {} bytes, format: {}",
            image_data.len(),
            format
        );

        let start_conversion = Instant::now();
        let image_data_owned = image_data.to_vec();
        let format_owned = format.to_string();

        let service_ref = Self {
            postgres: self.postgres.clone(),
            redis: self.redis.clone(),
            r2: self.r2.clone(),
        };

        let png_data = tokio::task::spawn_blocking(move || {
            if format_owned == "image/svg+xml" {
                service_ref.convert_svg_to_png(&image_data_owned, 1024, 1024)
            } else {
                service_ref.convert_to_png(&image_data_owned, 1024, 1024)
            }
        })
        .await
        .map_err(|e| AppError::InternalError(format!("Task join error: {}", e)))??;

        info!(
            "⏱️  Image conversion took: {:?}",
            start_conversion.elapsed()
        );

        info!("☁️  Loading AWS configuration");
        let aws_region = env::var("AWS_REGION").expect("AWS_REGION must be set");
        let region = Region::new(aws_region);
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(region)
            .load()
            .await;

        info!("📡 Creating Rekognition client");
        let client = Client::new(&config);

        let bytes = Bytes::from(png_data);
        let blob = Blob::new(bytes);
        let image = Image::builder().bytes(blob).build();

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

        let labels = resp.moderation_labels();
        let is_nsfw = self.is_adult_content(labels);

        info!("✅ NSFW check completed - Result: {}", is_nsfw);

        Ok(is_nsfw)
    }

    /// 성인물 여부를 판단하는 함수
    fn is_adult_content(&self, labels: &[aws_sdk_rekognition::types::ModerationLabel]) -> bool {
        let adult_categories = [
            ("Explicit", 10.0),
            ("Explicit Nudity", 10.0),
            ("Explicit Sexual Activity", 10.0),
            ("Exposed Buttocks or Anus", 10.0),
            ("Exposed Male Genitalia", 10.0),
            ("Exposed Female Genitalia", 10.0),
            ("Exposed Female Nipple", 10.0),
            ("Non-Explicit Nudity", 90.0),
        ];

        for label in labels {
            let name = label.name().unwrap_or_default();
            let confidence = label.confidence().unwrap_or_default();

            for (category, threshold) in &adult_categories {
                if name.to_lowercase() == category.to_lowercase() && confidence >= *threshold {
                    return true;
                }
            }
        }

        false
    }

    pub async fn process_and_upload_image(
        &self,
        image_data: &Bytes,
        content_type: &Option<String>,
    ) -> Result<UploadImageResponse, AppError> {
        let start_time = Instant::now();
        info!("🚀 Starting image processing");

        let image_id = Uuid::new_v4().to_string();

        info!("🔍 Validating image format");
        let validated_format = self.validate_image(image_data, content_type)?;
        info!("✅ Image format validated: {}", validated_format);

        let is_nsfw = self.check_nsfw(image_data, &validated_format).await?;

        let image_uri = self
            .r2
            .upload_metadata_image_file(&image_id, image_data, &validated_format)
            .await?;

        self.redis
            .set_nsfw_status(&image_uri, is_nsfw)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to cache NSFW status: {}", err))
            })?;

        let total_duration = start_time.elapsed();
        info!(
            "🎉 Image processing completed - Total time: {:?}, Image URI: {}, NSFW: {}",
            total_duration, image_uri, is_nsfw
        );

        Ok(UploadImageResponse { is_nsfw, image_uri })
    }

    pub async fn validate_metadata_request(
        &self,
        payload: &UploadMetadataRequest,
    ) -> Result<TokenMetadata, AppError> {
        let is_nsfw = match self
            .redis
            .get_nsfw_status(&payload.image_uri)
            .await
            .map_err(|_| {
                AppError::BadRequest("Failed to check NSFW status for this image".to_string())
            })? {
            Some(status) => status,
            None => {
                return Err(AppError::BadRequest(
                    "NSFW status not found for this image - please upload image first".to_string(),
                ));
            }
        };

        let metadata = TokenMetadata {
            name: payload.name.clone(),
            symbol: payload.symbol.clone(),
            description: payload.description.clone(),
            image_uri: payload.image_uri.clone(),
            website: payload.website.clone(),
            twitter: payload.twitter.clone(),
            telegram: payload.telegram.clone(),
            is_nsfw,
        };

        metadata.validate()?;

        Ok(metadata)
    }

    pub async fn upload_metadata(
        &self,
        metadata: TokenMetadata,
    ) -> Result<UploadMetadataResponse, AppError> {
        let metadata_id = Uuid::new_v4().to_string();
        let metadata_uri = self
            .r2
            .upload_metadata_file(&metadata_id, &metadata)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        MetadataController::new(self.postgres.clone())
            .save_token_metadata(&metadata, &metadata_uri)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        Ok(UploadMetadataResponse {
            metadata_uri,
            metadata,
        })
    }

    pub async fn get_gecko_metadata(
        &self,
        token_address: &str,
    ) -> Result<GeckoMetadataResponse, AppError> {
        // Try to get from Redis cache first
        if let Ok(Some(cached_response)) = self.redis.get_gecko_metadata(token_address).await {
            return Ok(cached_response);
        }

        // If not in cache, fetch from database
        let response = MetadataController::new(self.postgres.clone())
            .get_gecko_metadata(token_address)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        // Cache the result in Redis (ignore cache errors)
        let _ = self.redis.set_gecko_metadata(token_address, &response).await;

        Ok(response)
    }
}
