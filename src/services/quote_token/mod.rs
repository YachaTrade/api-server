use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::quote_token::QuoteTokenController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::quote_token::QuoteTokensResponse,
};

pub struct QuoteTokenService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl QuoteTokenService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn list(&self) -> Result<QuoteTokensResponse, AppError> {
        if let Ok(cached) = self.redis.get_quote_tokens_response().await {
            return Ok(cached);
        }

        let controller = QuoteTokenController::new(self.postgres.clone());
        let response = controller.list().await.map_err(|err| {
            error!("Failed to list quote tokens: {}", err);
            AppError::InternalError(err.to_string())
        })?;

        if let Err(err) = self.redis.set_quote_tokens_response(&response).await {
            error!("Failed to set quote tokens response: {}", err);
        }

        Ok(response)
    }
}
