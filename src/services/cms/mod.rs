use std::sync::Arc;

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
        let controller = CmsController::new(self.postgres.clone());

        // Check if user is admin
        let is_admin = controller.is_admin(session_address).await.map_err(|err| {
            AppError::InternalError(format!("Failed to check admin status: {}", err))
        })?;

        if !is_admin {
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        let response = controller.set_nsfw(request).await.map_err(|err| {
            AppError::InternalError(format!("Failed to set nsfw: {}", err))
        })?;

        Ok(response)
    }

    pub async fn insert_trend(
        &self,
        session_address: &str,
        request: InsertTrendRequest,
    ) -> Result<CmsActionResponse, AppError> {
        let controller = CmsController::new(self.postgres.clone());

        // Check if user is admin
        let is_admin = controller.is_admin(session_address).await.map_err(|err| {
            AppError::InternalError(format!("Failed to check admin status: {}", err))
        })?;

        if !is_admin {
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        let response = controller.insert_trend(request).await.map_err(|err| {
            AppError::InternalError(format!("Failed to insert trend: {}", err))
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
