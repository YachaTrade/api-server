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
            CmsActionResponse, DexTokenImageResponse, InsertTrendRequest, SetNsfwRequest,
            UpdateMetadataRequest, UpdateMetadataResponse,
        },
        metadata::TokenMetadata,
    },
    utils::{single_flight::GLOBAL_CACHE, valid_account_id},
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

        // Handle image upload if provided - generate new image URI
        if let Some(image_bytes) = image_data {
            let image_uri = self.upload_new_image(&image_bytes).await?;
            metadata.image_uri = image_uri;
        }

        // Upload updated metadata to the same URL
        self.upload_metadata_to_uri(&metadata_uri, &metadata)
            .await?;

        // Update token table in DB
        controller
            .update_token_metadata(
                &request.token_id,
                metadata.description.as_deref(),
                Some(&metadata.image_uri),
                metadata.website.as_deref(),
                metadata.twitter.as_deref(),
                metadata.telegram.as_deref(),
            )
            .await
            .map_err(|err| {
                error!("Failed to update token table: {}", err);
                AppError::InternalError(format!("Failed to update token: {}", err))
            })?;

        info!("Updated token table for: {}", request.token_id);

        Ok(UpdateMetadataResponse {
            success: true,
            metadata_uri,
        })
    }

    /// dex_token 로고 이미지 업로드 + image_uri 등록 (admin).
    /// 이미지를 R2(`coin/{uuid}`)에 올리고 dex_token.image_uri를 갱신한다.
    pub async fn update_dex_token_image(
        &self,
        session_address: &str,
        token_id: &str,
        image_data: Bytes,
    ) -> Result<DexTokenImageResponse, AppError> {
        // dex 토큰은 베니티 접미사 없음 → 순수 EIP-55 체크섬 정규화
        let token_id = valid_account_id(token_id)
            .ok_or_else(|| AppError::BadRequest("Invalid token_id format".to_string()))?;

        let controller = CmsController::new(self.postgres.clone());

        // Verify admin status on the PRIMARY (write pool). write 경로 인가를 replica에서
        // 읽으면 복제 지연 동안 방금 권한 회수된 admin이 통과할 수 있어 primary로 확인한다.
        let is_admin = controller
            .verify_admin_on_writer(session_address)
            .await
            .map_err(|err| AppError::InternalError(format!("Failed to verify admin: {}", err)))?;
        if !is_admin {
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        // 존재 확인을 업로드 前에 → 미존재 시 R2 orphan 방지
        let exists = controller
            .dex_token_exists(&token_id)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to check dex_token: {}", err))
            })?;
        if !exists {
            return Err(AppError::NotFound(format!(
                "dex_token not found: {}",
                token_id
            )));
        }

        // 이미지 검증(magic byte) + R2 업로드 → coin/{uuid}
        let image_uri = self.upload_new_image(&image_data).await?;

        // 최종 UPDATE에 admin EXISTS를 원자적으로 묶음 → 업로드 동안 권한이 회수되는 TOCTOU 창 차단.
        let updated = controller
            .set_dex_token_image(session_address, &token_id, &image_uri)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to set dex_token image: {}", err))
            })?;
        if !updated {
            // 사전 admin/존재 확인 통과 후 0건 → 업로드 중 admin 회수(또는 토큰 삭제).
            // 원자적 admin 가드에서 막힌 것이므로 인가 실패로 처리.
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        info!("Updated dex_token image for {}: {}", token_id, image_uri);

        Ok(DexTokenImageResponse {
            success: true,
            token_id,
            image_uri,
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

    async fn upload_new_image(&self, image_data: &Bytes) -> Result<String, AppError> {
        // Validate image format by magic bytes
        let validated_format = self.validate_image(image_data)?;

        // Generate new UUID for image
        let image_id = uuid::Uuid::new_v4().to_string();

        // Upload with new image_id and validated format
        let image_uri = self
            .r2
            .upload_metadata_image_file(&image_id, image_data, &validated_format)
            .await
            .map_err(|e| {
                error!("Failed to upload image: {}", e);
                AppError::InternalError(format!("Failed to upload image: {}", e))
            })?;

        info!("Uploaded new image to: {}", image_uri);
        Ok(image_uri)
    }

    fn validate_image(&self, data: &[u8]) -> Result<String, AppError> {
        const ALLOWED_IMAGE_TYPES: [&str; 4] =
            ["image/jpeg", "image/png", "image/webp", "image/svg+xml"];

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

        if !ALLOWED_IMAGE_TYPES.contains(&actual_format) {
            return Err(AppError::BadRequest(format!(
                "Unsupported image type: {}",
                actual_format
            )));
        }

        Ok(actual_format.to_string())
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
