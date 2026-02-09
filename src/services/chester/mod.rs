use std::sync::Arc;

use crate::{
    controllers::chester::ChesterController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::chester::{ChesterInfoResponse, ChesterRewardsResponse, ChesterVolumeResponse},
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
