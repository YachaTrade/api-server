use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::trading::position::PositionController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        profile::HoldTokenResponse,
        trading::position::TokenHolderResponse,
    },
};

pub struct PositionService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl PositionService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_token_holder_response(token_id, pagination)
            .await
        {
            return Ok(cached);
        }

        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_holders_by_token(token_id, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self
            .redis
            .set_token_holder_response(token_id, &response, pagination)
            .await
        {
            warn!(
                "Failed to set token holder cache: token_id={}, error={}",
                token_id, err
            );
        }

        Ok(response)
    }

    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_account_hold_token(account_id, pagination)
            .await
        {
            return Ok(cached);
        }

        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_hold_token_by_account(account_id, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self
            .redis
            .set_account_hold_token(account_id, pagination, &response)
            .await
        {
            warn!(
                "Failed to set hold token cache: account_id={}, error={}",
                account_id, err
            );
        }

        Ok(response)
    }
}
