use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::vault::VaultController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::vault::TokenVaultsResponse,
};

pub struct VaultService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl VaultService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_token_vaults(
        &self,
        token_id: &str,
    ) -> Result<TokenVaultsResponse, AppError> {
        if let Ok(cached) = self.redis.get_token_vaults_response(token_id).await {
            return Ok(cached);
        }

        let controller = VaultController::new(self.postgres.clone());
        let response = controller.get_token_vaults(token_id).await.map_err(|err| {
            error!(
                "Failed to get token vaults: token_id: {}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;

        if let Err(err) = self
            .redis
            .set_token_vaults_response(token_id, &response)
            .await
        {
            error!("Failed to set token vaults response: {}", err);
        }

        Ok(response)
    }
}
