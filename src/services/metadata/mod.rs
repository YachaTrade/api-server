use bytes::Bytes;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    controllers::metadata::MetadataController,
    db::{R2::R2Client, postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::metadata::{
        TokenMetadata, UploadImageResponse, UploadMetadataRequest, UploadMetadataResponse,
    },
};

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

    pub async fn upload_image(
        &self,
        image_id: &str,
        image_data: &Bytes,
        validated_format: &str,
        is_nsfw: bool,
    ) -> Result<UploadImageResponse, AppError> {
        let image_uri = self
            .r2
            .upload_metadata_image_file(image_id, image_data, validated_format)
            .await?;

        self.redis
            .set_nsfw_status(&image_uri, is_nsfw)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to cache NSFW status: {}", err))
            })?;

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
        let metadata_url = self
            .r2
            .upload_metadata_file(&metadata_id, &metadata)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        MetadataController::new(self.postgres.clone())
            .save_token_metadata(&metadata, &metadata_url)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        Ok(UploadMetadataResponse {
            metadata_url,
            metadata,
        })
    }
}
