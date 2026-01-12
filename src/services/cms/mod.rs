use std::{str::FromStr, sync::Arc};

use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::sol;
use bytes::Bytes;
use tracing::{error, info};

use crate::{
    config::RPC_URL,
    controllers::cms::CmsController,
    db::{postgres::PostgresDatabase, r2::R2Client},
    result::AppError,
    types::{
        cms::{
            CmsActionResponse, InsertTrendRequest, SetNsfwRequest, UpdateMetadataRequest,
            UpdateMetadataResponse,
        },
        metadata::TokenMetadata,
    },
    utils::single_flight::GLOBAL_CACHE,
};

sol! {
    #[allow(missing_docs)]
    #[sol(rpc)]
    interface IToken {
        function tokenURI() external view returns (string);
    }
}

pub struct CmsService {
    postgres: Arc<PostgresDatabase>,
    r2: Arc<R2Client>,
}

impl CmsService {
    pub fn new(postgres: Arc<PostgresDatabase>, r2: Arc<R2Client>) -> Self {
        Self { postgres, r2 }
    }

    pub async fn set_nsfw(
        &self,
        session_address: &str,
        request: SetNsfwRequest,
    ) -> Result<CmsActionResponse, AppError> {
        // Validate token_id is a valid EVM address
        Address::from_str(&request.token_id)
            .map_err(|_| AppError::BadRequest("Invalid token_id format".to_string()))?;

        let controller = CmsController::new(self.postgres.clone());

        // Use atomic admin check + operation to prevent TOCTOU attacks
        let response = controller
            .set_nsfw_with_admin_check(session_address, request)
            .await
            .map_err(|err| {
                let err_msg = err.to_string();
                if err_msg.contains("Admin access required") {
                    AppError::AuthError("Admin access required".to_string())
                } else {
                    AppError::InternalError(format!("Failed to set nsfw: {}", err))
                }
            })?;

        Ok(response)
    }

    pub async fn insert_trend(
        &self,
        session_address: &str,
        request: InsertTrendRequest,
    ) -> Result<CmsActionResponse, AppError> {
        // Validate all token_ids are valid EVM addresses
        for token_id in &request.token_ids {
            Address::from_str(token_id).map_err(|_| {
                AppError::BadRequest(format!("Invalid token_id format: {}", token_id))
            })?;
        }

        let controller = CmsController::new(self.postgres.clone());

        // Use atomic admin check + operation to prevent TOCTOU attacks
        let response = controller
            .insert_trend_with_admin_check(session_address, request)
            .await
            .map_err(|err| {
                let err_msg = err.to_string();
                if err_msg.contains("Admin access required") {
                    AppError::AuthError("Admin access required".to_string())
                } else {
                    AppError::InternalError(format!("Failed to insert trend: {}", err))
                }
            })?;

        // Invalidate trend cache
        self.invalidate_trend_cache().await;

        Ok(response)
    }

    async fn invalidate_trend_cache(&self) {
        GLOBAL_CACHE
            .cache
            .invalidate(&"trend:service:all".to_string())
            .await;

        GLOBAL_CACHE
            .cache
            .invalidate(&"trend_tokens:all".to_string())
            .await;
    }

    pub async fn update_metadata(
        &self,
        session_address: &str,
        request: UpdateMetadataRequest,
        image_data: Option<Bytes>,
        image_content_type: Option<String>,
    ) -> Result<UpdateMetadataResponse, AppError> {
        // Validate token_id is a valid EVM address
        let token_address: Address = Address::from_str(&request.token_id)
            .map_err(|_| AppError::BadRequest("Invalid token_id format".to_string()))?;

        let controller = CmsController::new(self.postgres.clone());

        // Verify admin status
        let is_admin = controller
            .verify_admin(session_address)
            .await
            .map_err(|err| AppError::InternalError(format!("Failed to verify admin: {}", err)))?;

        if !is_admin {
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        // Get tokenURI from contract
        let metadata_uri = self.get_token_uri(&token_address).await?;
        info!("Fetched tokenURI: {}", metadata_uri);

        // Fetch current metadata from URL
        let mut metadata = self.fetch_metadata(&metadata_uri).await?;
        info!("Fetched current metadata: {:?}", metadata);

        // Update fields if provided
        if let Some(description) = request.description {
            metadata.description = Some(description);
        }
        if let Some(website) = request.website {
            metadata.website = Some(website);
        }
        if let Some(twitter) = request.twitter {
            metadata.twitter = Some(twitter);
        }
        if let Some(telegram) = request.telegram {
            metadata.telegram = Some(telegram);
        }

        // Handle image upload if provided
        if let Some(image_bytes) = image_data {
            let content_type = image_content_type.unwrap_or_else(|| "image/png".to_string());
            let image_uri = self
                .upload_and_replace_image(&metadata.image_uri, &image_bytes, &content_type)
                .await?;
            metadata.image_uri = image_uri;
        }

        // Upload updated metadata to the same URL
        self.upload_metadata_to_uri(&metadata_uri, &metadata)
            .await?;

        Ok(UpdateMetadataResponse {
            success: true,
            metadata_uri,
        })
    }

    async fn get_token_uri(&self, token_address: &Address) -> Result<String, AppError> {
        let rpc_url: url::Url = RPC_URL
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid RPC_URL: {}", e)))?;

        let provider = ProviderBuilder::new().connect_http(rpc_url);

        let token = IToken::new(*token_address, provider);

        let token_uri: String = token.tokenURI().call().await.map_err(|e| {
            error!("Failed to call tokenURI: {}", e);
            AppError::InternalError(format!("Failed to get tokenURI: {}", e))
        })?;

        Ok(token_uri)
    }

    async fn fetch_metadata(&self, metadata_uri: &str) -> Result<TokenMetadata, AppError> {
        let response = reqwest::get(metadata_uri).await.map_err(|e| {
            error!("Failed to fetch metadata from {}: {}", metadata_uri, e);
            AppError::InternalError(format!("Failed to fetch metadata: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(AppError::InternalError(format!(
                "Failed to fetch metadata: HTTP {}",
                response.status()
            )));
        }

        let metadata: TokenMetadata = response.json().await.map_err(|e| {
            error!("Failed to parse metadata JSON: {}", e);
            AppError::InternalError(format!("Failed to parse metadata: {}", e))
        })?;

        Ok(metadata)
    }

    async fn upload_and_replace_image(
        &self,
        current_image_uri: &str,
        image_data: &Bytes,
        content_type: &str,
    ) -> Result<String, AppError> {
        // Extract image_id from current URI (e.g., https://storage.nadapp.net/coin/UUID)
        let image_id = current_image_uri
            .strip_prefix("https://storage.nadapp.net/coin/")
            .ok_or_else(|| AppError::BadRequest("Invalid current image URI format".to_string()))?;

        // Upload to the same path to overwrite
        let image_uri = self
            .r2
            .upload_metadata_image_file(image_id, image_data, content_type)
            .await
            .map_err(|e| {
                error!("Failed to upload image: {}", e);
                AppError::InternalError(format!("Failed to upload image: {}", e))
            })?;

        info!("Uploaded image to: {}", image_uri);
        Ok(image_uri)
    }

    async fn upload_metadata_to_uri(
        &self,
        metadata_uri: &str,
        metadata: &TokenMetadata,
    ) -> Result<(), AppError> {
        // Extract metadata_id from URI (e.g., https://storage.nadapp.net/metadata/UUID.json)
        let metadata_id = metadata_uri
            .strip_prefix("https://storage.nadapp.net/metadata/")
            .and_then(|s| s.strip_suffix(".json"))
            .ok_or_else(|| AppError::BadRequest("Invalid metadata URI format".to_string()))?;

        // Upload to the same path to overwrite
        self.r2
            .upload_metadata_file(metadata_id, metadata)
            .await
            .map_err(|e| {
                error!("Failed to upload metadata: {}", e);
                AppError::InternalError(format!("Failed to upload metadata: {}", e))
            })?;

        info!("Uploaded metadata to: {}", metadata_uri);
        Ok(())
    }
}
