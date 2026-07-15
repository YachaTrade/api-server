use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::token::gift_fee::GiftFeeController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    services::capricorn::{CAPRICORN_UNION_CAP, CapricornClient, lp_amounts_by_token},
    types::{common::pagination::PaginationParams, profile::GiftFeeTokensResponse},
};

pub struct GiftFeeService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
    capricorn: Arc<CapricornClient>,
}

impl GiftFeeService {
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

    pub async fn get_gift_fee_tokens(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<GiftFeeTokensResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_account_gift_fee(account_id, pagination)
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

        let controller = GiftFeeController::new(self.postgres.clone());
        let response = controller
            .get_gift_fee_tokens(account_id, pagination, &v1_lp)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get gift fee tokens: account_id: {}, error: {}",
                    account_id, err
                );
                AppError::InternalError(err.to_string())
            })?;

        if let Err(err) = self
            .redis
            .set_account_gift_fee(account_id, pagination, &response)
            .await
        {
            error!(
                "Failed to set gift fee tokens: account_id: {}, error: {}",
                account_id, err
            );
        }

        Ok(response)
    }
}
