use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::token::create::TokenCreatedController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    services::capricorn::{CAPRICORN_UNION_CAP, CapricornClient, lp_amounts_by_token},
    types::{common::pagination::PaginationParams, profile::CreatedTokensResponse},
};

pub struct TokenCreatedService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
    capricorn: Arc<CapricornClient>,
}

impl TokenCreatedService {
    pub fn new(
        postgres: Arc<PostgresDatabase>,
        redis: Arc<RedisDatabase>,
        capricorn: Arc<CapricornClient>,
    ) -> Self {
        Self {
            postgres,
            redis,
            capricorn,
        }
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

        let positions = self.capricorn.cached_fetch_by_owner(account_id).await;
        let mut v1_lp = lp_amounts_by_token(&positions);
        if v1_lp.len() > CAPRICORN_UNION_CAP {
            tracing::warn!(
                "capricorn owner LP rows {} exceed cap; V1 LP omitted for this response",
                v1_lp.len()
            );
            v1_lp.clear();
        }

        let controller = TokenCreatedController::new(self.postgres.clone());
        let response = controller
            .get_tokens_created(account_id, pagination, &v1_lp)
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
