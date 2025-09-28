use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::trading::chart::ChartController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::trading::chart::{BarResponse, GetBarsRequest},
};

pub struct ChartService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl ChartService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_prices(
        &self,
        token_id: &str,
        request: &GetBarsRequest,
    ) -> Result<BarResponse, AppError> {
        if let Ok(Some(cached)) = self.redis.get_prices(token_id, request).await {
            return Ok(cached);
        }

        let controller = ChartController::new(self.postgres.clone());
        let response = controller
            .get_prices(token_id, request)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self.redis.set_prices(token_id, request, &response).await {
            warn!("Failed to cache bar data response: {}", err);
        }

        Ok(response)
    }
}
