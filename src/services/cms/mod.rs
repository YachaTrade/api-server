use std::{str::FromStr, sync::Arc};

use alloy::primitives::Address;

use crate::{
    controllers::cms::CmsController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::cms::{CmsActionResponse, InsertTrendRequest, SetNsfwRequest},
    utils::single_flight::GLOBAL_CACHE,
};

pub struct CmsService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl CmsService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
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
            Address::from_str(token_id)
                .map_err(|_| AppError::BadRequest(format!("Invalid token_id format: {}", token_id)))?;
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
}
