use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::trend::TrendController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::trend::{TrendActionResponse, TrendRequest, TrendResponse},
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct TrendService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl TrendService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_trend(&self) -> Result<TrendResponse, AppError> {
        // Try Redis cache first
        if let Ok(cached) = self.redis.get_trend_response().await {
            return Ok(cached);
        }

        // Use single flight to prevent concurrent DB calls
        let cache_key = "trend:service:all";
        let postgres = self.postgres.clone();
        let redis = self.redis.clone();

        let response = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let postgres = postgres.clone();
            let redis = redis.clone();
            async move {
                let controller = TrendController::new(postgres);
                let response = controller.get_trend_tokens_raw().await?;

                // Cache to Redis
                if let Err(err) = redis.set_trend_response(&response).await {
                    error!("Failed to set trend response to Redis: {}", err);
                }

                Ok(response)
            }
        })
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to get trend tokens: {}", err)))?;

        Ok(response)
    }

    pub async fn insert_trend(
        &self,
        session_address: &str,
        payload: TrendRequest,
    ) -> Result<TrendActionResponse, AppError> {
        let controller = TrendController::new(self.postgres.clone());

        // Check if user is admin
        let is_admin = controller.is_admin(session_address).await.map_err(|err| {
            AppError::InternalError(format!("Failed to check admin status: {}", err))
        })?;

        if !is_admin {
            return Err(AppError::AuthError("Admin access required".to_string()));
        }

        let response = controller
            .insert_trend_token(payload)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to insert trend token: {}", err))
            })?;

        // Invalidate caches after insert
        self.invalidate_trend_cache().await;

        Ok(response)
    }

    async fn invalidate_trend_cache(&self) {
        // Invalidate single flight cache
        GLOBAL_CACHE
            .cache
            .invalidate(&"trend:service:all".to_string())
            .await;
    }
}
