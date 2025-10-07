use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::token::create::TokenCreatedController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{common::pagination::PaginationParams, profile::CreatedTokensResponse},
};

pub struct TokenCreatedService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl TokenCreatedService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<CreatedTokensResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_account_token_created(account_id, pagination)
            .await
        {
            return Ok(cached);
        }

        let controller = TokenCreatedController::new(self.postgres.clone());
        let response = controller
            .get_tokens_created(account_id, pagination)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get token created: account_id: {}, error: {}",
                    account_id, err
                );
                AppError::InternalError(err.to_string())
            })?;

        if let Err(err) = self
            .redis
            .set_account_token_created(account_id, pagination, &response)
            .await
        {
            error!(
                "Failed to set token created: account_id: {}, error: {}",
                account_id, err
            );
        }

        Ok(response)
    }
}
