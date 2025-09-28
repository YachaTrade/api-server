use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::new_content::NewContentController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::new_content::NewContentResponse,
};

pub struct NewContentService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl NewContentService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_new_content(&self) -> Result<NewContentResponse, AppError> {
        if let Ok(cached_response) = self.redis.get_new_content().await {
            return Ok(cached_response);
        }

        let controller = NewContentController::new(self.postgres.clone());
        let response = controller.get_new_content().await.map_err(|err| {
            AppError::InternalError(format!("Failed to get new content: {}", err))
        })?;

        if let Err(err) = self.redis.set_new_content(&response).await {
            warn!("Failed to set new content cache: {}", err);
        }

        Ok(response)
    }
}
