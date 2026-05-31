use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::trading::position::PositionController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    services::capricorn::{
        CAPRICORN_UNION_CAP, CapricornClient, lp_amounts_by_owner, lp_amounts_by_token,
    },
    types::{
        common::pagination::PaginationParams, profile::HoldTokenResponse,
        trading::position::TokenHolderResponse,
    },
};

pub struct PositionService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
    capricorn: Arc<CapricornClient>,
}

impl PositionService {
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

        let positions = self.capricorn.cached_fetch_by_token(token_id).await;
        let mut v1_lp = lp_amounts_by_owner(&positions, token_id);
        if v1_lp.len() > CAPRICORN_UNION_CAP {
            tracing::warn!(
                "capricorn token LP owners {} exceed cap; V1 LP omitted for this response",
                v1_lp.len()
            );
            v1_lp.clear();
        }
        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_holders_by_token(token_id, pagination, &v1_lp)
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

        let positions = self.capricorn.cached_fetch_by_owner(account_id).await;
        let mut v1_lp = lp_amounts_by_token(&positions);
        if v1_lp.len() > CAPRICORN_UNION_CAP {
            tracing::warn!(
                "capricorn owner LP rows {} exceed cap; V1 LP omitted for this response",
                v1_lp.len()
            );
            v1_lp.clear();
        }
        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_hold_token_by_account(account_id, pagination, &v1_lp)
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
