use bytes::Bytes;
use std::{sync::Arc, time::Instant};
use tracing::info;
use uuid::Uuid;

use crate::{
    controllers::metadata::MetadataController,
    db::{postgres::PostgresDatabase, r2::R2Client, redis::RedisDatabase},
    result::AppError,
    services::moderation::check_nsfw,
    types::metadata::{
        TerminalMetadataResponse, TokenMetadata, UploadImageResponse, UploadMetadataRequest,
        UploadMetadataResponse,
    },
    utils::image::sniff_image_format,
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

    /// Validate image by detecting actual format from magic bytes
    fn validate_image(
        &self,
        data: &[u8],
        _content_type: &Option<String>,
    ) -> Result<String, AppError> {
        sniff_image_format(data, &ALLOWED_IMAGE_TYPES).map(str::to_string)
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

        let is_nsfw = check_nsfw(image_data, &validated_format).await?;

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

    pub async fn get_terminal_metadata(
        &self,
        token_address: &str,
    ) -> Result<TerminalMetadataResponse, AppError> {
        // Try to get from Redis cache first
        if let Ok(Some(cached_response)) = self.redis.get_terminal_metadata(token_address).await {
            return Ok(cached_response);
        }

        // If not in cache, fetch from database
        let response = MetadataController::new(self.postgres.clone())
            .get_terminal_metadata(token_address)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        // Cache the result in Redis (ignore cache errors)
        let _ = self
            .redis
            .set_terminal_metadata(token_address, &response)
            .await;

        Ok(response)
    }
}
