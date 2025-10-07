use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::trading::market::MarketController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::trading::market::MarketResponse,
};

pub struct MarketService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl MarketService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_market(&self, token_id: &str) -> Result<MarketResponse, AppError> {
        if let Ok(cached) = self.redis.get_market(token_id).await {
            return Ok(cached);
        }

        let controller = MarketController::new(self.postgres.clone());
        let market = controller
            .get_market_by_token(token_id)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self.redis.set_market(token_id, &market).await {
            warn!(
                "Failed to set market cache: token_id={}, error={}",
                token_id, err
            );
        }

        Ok(market)
    }
}
