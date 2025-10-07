use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::trading::swap_history::SwapController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        profile::SwapHistoryResponse,
        trading::swap_history::{SwapQuery, TokenSwapResponse},
    },
};

pub struct SwapService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl SwapService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_swaps_by_token(
        &self,
        token_id: &str,
        query: &SwapQuery,
    ) -> Result<TokenSwapResponse, AppError> {
        if let Ok(cached) = self.redis.get_token_swap_history(token_id, query).await {
            return Ok(cached);
        }

        let controller = SwapController::new(self.postgres.clone());
        let response = controller
            .get_swaps_by_token(token_id, query)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self
            .redis
            .set_token_swap_history(token_id, &response, query)
            .await
        {
            warn!(
                "Failed to set token swap history cache: token_id={}, error={}",
                token_id, err
            );
        }

        Ok(response)
    }

    pub async fn get_swaps_by_account(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<SwapHistoryResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_account_swap_history(account_id, &pagination)
            .await
        {
            return Ok(cached);
        }

        let controller = SwapController::new(self.postgres.clone());
        let response = controller
            .get_swaps_by_account(account_id, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Err(err) = self
            .redis
            .set_account_swap_history(account_id, &pagination, &response)
            .await
        {
            warn!(
                "Failed to set account swap history cache: account_id={}, error={}",
                account_id, err
            );
        }

        Ok(response)
    }
}
