use std::sync::Arc;

use crate::{
    controllers::chester::ChesterController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{
        chester::{
            ChesterBoxRewardsResponse, ChesterInfoResponse, ChesterRewardsResponse,
            ChesterVolumeResponse,
        },
        profile::SwapHistoryResponse,
    },
};

pub struct ChesterService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl ChesterService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_rewards(&self) -> Result<ChesterRewardsResponse, AppError> {
        if let Ok(Some(cached)) = self.redis.get_chester_rewards().await {
            return Ok(cached);
        }

        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_rewards()
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let _ = self.redis.set_chester_rewards(&response).await;

        Ok(response)
    }

    pub async fn get_round(&self) -> Result<Option<ChesterInfoResponse>, AppError> {
        if let Ok(Some(cached)) = self.redis.get_chester_round().await {
            return Ok(Some(cached));
        }

        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_round()
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        if let Some(ref data) = response {
            let _ = self.redis.set_chester_round(data).await;
        }

        Ok(response)
    }

    pub async fn get_swap_history(
        &self,
        account_id: &str,
        page: i64,
        limit: i64,
    ) -> Result<SwapHistoryResponse, AppError> {
        if let Ok(Some(cached)) = self
            .redis
            .get_chester_swap_history(account_id, page, limit)
            .await
        {
            return Ok(cached);
        }

        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_swap_history(account_id, page, limit)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let _ = self
            .redis
            .set_chester_swap_history(account_id, page, limit, &response)
            .await;

        Ok(response)
    }

    pub async fn get_box_rewards(
        &self,
        account_id: &str,
        round: Option<i64>,
    ) -> Result<ChesterBoxRewardsResponse, AppError> {
        // For cache, we need to resolve the round first
        // If round is provided, use it for cache lookup; otherwise skip cache
        if let Some(r) = round {
            if let Ok(Some(cached)) = self.redis.get_chester_box_rewards(account_id, r).await {
                return Ok(cached);
            }
        }

        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_box_rewards(account_id, round)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        // Cache with the actual round from the response
        if let Some(first) = response.rewards.first() {
            let _ = self
                .redis
                .set_chester_box_rewards(account_id, first.round, &response)
                .await;
        } else if let Some(r) = round {
            // Even empty results can be cached if round was specified
            let _ = self
                .redis
                .set_chester_box_rewards(account_id, r, &response)
                .await;
        }

        Ok(response)
    }

    pub async fn get_volume(&self, account_id: &str) -> Result<ChesterVolumeResponse, AppError> {
        if let Ok(Some(cached)) = self.redis.get_chester_volume(account_id).await {
            return Ok(cached);
        }

        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_volume(account_id)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let _ = self.redis.set_chester_volume(account_id, &response).await;

        Ok(response)
    }
}
