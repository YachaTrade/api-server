use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::new_event::NewEventController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::new_event::NewEventResponse,
};

pub struct NewEventService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl NewEventService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_new_events(&self) -> Result<NewEventResponse, AppError> {
        if let Ok(cached_response) = self.redis.get_new_event().await {
            return Ok(cached_response);
        }

        let controller = NewEventController::new(self.postgres.clone());
        let response = controller
            .get_new_events()
            .await
            .map_err(|err| AppError::InternalError(format!("Failed to get new events: {}", err)))?;

        if let Err(err) = self.redis.set_new_event(&response).await {
            warn!("Failed to set new event cache: {}", err);
        }

        Ok(response)
    }
}
