use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::token::metadata::TokenMetadataController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::token::metadata::TokenMetadataResponse,
};

pub struct TokenMetadataService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl TokenMetadataService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_token_metadata(
        &self,
        token_id: &str,
    ) -> Result<TokenMetadataResponse, AppError> {
        if let Ok(cached) = self.redis.get_token_metadata(token_id).await {
            return Ok(cached);
        }

        let controller = TokenMetadataController::new(self.postgres.clone());
        let response = controller
            .get_token_metadata(token_id)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get token metadata: token_id: {}, error: {}",
                    token_id, err
                );
                AppError::InternalError(err.to_string())
            })?;

        if let Err(err) = self.redis.set_token_metadata(token_id, &response).await {
            error!("Failed to set token metadata: {}", err);
        }

        Ok(response)
    }
}
