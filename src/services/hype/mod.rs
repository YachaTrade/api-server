use std::sync::Arc;

use chrono::Timelike;
use tracing::error;

use crate::{
    controllers::hype::HypeController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        hype::{
            AmountResponse, HypeEpochResponse, HypePointResponse,
            HypeRewardAddHistoryResponse, HypeTokenQuery,
            HypeTokenResponse, HypeVoteHistoryResponse, HypeVoteRequest, HypeVoteResponse,
        },
        profile::PointHistoryResponse,
    },
};

pub struct HypeService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl HypeService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_hype_token(
        &self,
        query: HypeTokenQuery,
    ) -> Result<HypeTokenResponse, AppError> {
        let controller = HypeController::new(self.postgres.clone());

        match query.epoch {
            Some(epoch) => {
                if let Ok(cached) = self.redis.get_hype_token_epoch_response(epoch).await {
                    return Ok(cached);
                }

                let response = controller
                    .get_hype_token_epoch(epoch)
                    .await
                    .map_err(|err| {
                        AppError::InternalError(format!(
                            "Failed to get hype token for epoch {}, error: {}",
                            epoch, err
                        ))
                    })?;

                if let Err(err) = self
                    .redis
                    .set_hype_token_epoch_response(epoch, &response)
                    .await
                {
                    error!("Failed to set hype token epoch response: {}", err);
                }

                Ok(response)
            }
            None => {
                if let Ok(cached) = self.redis.get_hype_token_response().await {
                    return Ok(cached);
                }

                let response = controller.get_hype_token().await.map_err(|err| {
                    AppError::InternalError(format!("Failed to get hype token, error: {}", err))
                })?;

                if let Err(err) = self.redis.set_hype_token_response(&response).await {
                    error!("Failed to set hype token response: {}", err);
                }

                Ok(response)
            }
        }
    }

    pub async fn get_hype_token_latest(&self) -> Result<HypeTokenResponse, AppError> {
        if let Ok(cached) = self.redis.get_hype_token_latest_response().await {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller.get_hype_token_latest().await.map_err(|err| {
            AppError::InternalError(format!("Failed to get latest hype token, error: {}", err))
        })?;

        if let Err(err) = self.redis.set_hype_token_latest_response(&response).await {
            error!("Failed to set latest hype token response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_hype_point(&self, account_id: &str) -> Result<HypePointResponse, AppError> {
        if let Ok(cached) = self.redis.get_hype_point_response(account_id).await {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller.get_hype_point(account_id).await.map_err(|err| {
            AppError::InternalError(format!("Failed to get hype point, error: {}", err))
        })?;

        if let Err(err) = self
            .redis
            .set_hype_point_response(account_id, &response)
            .await
        {
            error!("Failed to set hype point response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_hype_epoch(&self) -> Result<HypeEpochResponse, AppError> {
        let now = chrono::Utc::now();
        let is_midnight_utc = now.hour() == 0 && now.minute() == 0;

        if !is_midnight_utc {
            if let Ok(cached) = self.redis.get_hype_epoch_response().await {
                return Ok(cached);
            }
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller.get_hype_epoch().await.map_err(|err| {
            AppError::InternalError(format!("Failed to get hype epoch, error: {}", err))
        })?;

        if let Err(err) = self.redis.set_hype_epoch_response(&response).await {
            error!("Failed to set hype token response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_hype_vote_history(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<HypeVoteHistoryResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_hype_vote_history_response(account_id, params)
            .await
        {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller
            .get_hype_vote_history(account_id, params)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to get hype vote history, error: {}", err))
            })?;

        if let Err(err) = self
            .redis
            .set_hype_vote_history_response(account_id, params, &response)
            .await
        {
            error!("Failed to set hype vote history response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_hype_point_history(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<PointHistoryResponse, AppError> {
        let controller = HypeController::new(self.postgres.clone());
        let response = controller
            .get_hype_point_history(account_id, params)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to get hype point history, error: {}", err))
            })?;

        if let Err(err) = self
            .redis
            .set_hype_point_history_response(account_id, params, &response)
            .await
        {
            error!("Failed to set hype point history response: {}", err);
        }

        Ok(response)
    }


    pub async fn get_hype_reward_add_history(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<HypeRewardAddHistoryResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_hype_reward_add_history_response(account_id, params)
            .await
        {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller
            .get_hype_reward_add_history(account_id, params)
            .await
            .map_err(|err| {
                AppError::InternalError(format!(
                    "Failed to get hype reward add history, error: {}",
                    err
                ))
            })?;

        if let Err(err) = self
            .redis
            .set_hype_reward_add_history_response(account_id, params, &response)
            .await
        {
            error!("Failed to set hype reward add history response: {}", err);
        }

        Ok(response)
    }

    pub async fn vote(
        &self,
        account_id: &str,
        payload: &HypeVoteRequest,
    ) -> Result<HypeVoteResponse, AppError> {
        let controller = HypeController::new(self.postgres.clone());

        let active_epoch = controller.get_active_epoch().await.map_err(|err| {
            AppError::InternalError(format!("Failed to verify active epoch: {}", err))
        })?;

        if active_epoch.is_none() {
            return Err(AppError::BadRequest(
                "No active hype epoch available".to_string(),
            ));
        }

        let response = controller
            .vote(account_id, payload)
            .await
            .map_err(|err| AppError::InternalError(format!("Failed to vote, error: {}", err)))?;

        // Invalidate hype_token cache after successful vote
        if let Err(err) = self.redis.delete_hype_token_cache().await {
            error!("Failed to delete hype token cache: {}", err);
        }

        Ok(response)
    }

    pub async fn get_community_treasury(&self) -> Result<AmountResponse, AppError> {
        if let Ok(cached) = self.redis.get_community_treasury_response().await {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller.get_community_treasury().await.map_err(|err| {
            AppError::InternalError(format!("Failed to get community treasury, error: {}", err))
        })?;

        if let Err(err) = self.redis.set_community_treasury_response(&response).await {
            error!("Failed to set community treasury response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_total_hype_point(&self) -> Result<AmountResponse, AppError> {
        if let Ok(cached) = self.redis.get_total_hype_point_response().await {
            return Ok(cached);
        }

        let controller = HypeController::new(self.postgres.clone());
        let response = controller.get_total_hype_point().await.map_err(|err| {
            AppError::InternalError(format!("Failed to get total hype point, error: {}", err))
        })?;

        if let Err(err) = self.redis.set_total_hype_point_response(&response).await {
            error!("Failed to set total hype point response: {}", err);
        }

        Ok(response)
    }
}
