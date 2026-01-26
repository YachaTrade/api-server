use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::leaderboard::LeaderboardController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::leaderboard::{HypePointLeaderboardResponse, LeaderboardQuery, PnlLeaderboardResponse},
};

pub struct LeaderboardService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl LeaderboardService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_hype_point_leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<HypePointLeaderboardResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_hype_point_leaderboard_response(query.page, query.limit)
            .await
        {
            return Ok(cached);
        }

        let controller = LeaderboardController::new(self.postgres.clone());
        let response = controller
            .get_hype_point_leaderboard(query)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to get hype point leaderboard: {}", err))
            })?;

        if let Err(err) = self
            .redis
            .set_hype_point_leaderboard_response(query.page, query.limit, &response)
            .await
        {
            error!("Failed to set hype point leaderboard response: {}", err);
        }

        Ok(response)
    }

    pub async fn get_pnl_leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<PnlLeaderboardResponse, AppError> {
        if let Ok(cached) = self
            .redis
            .get_pnl_leaderboard_response(query.page, query.limit)
            .await
        {
            return Ok(cached);
        }

        let controller = LeaderboardController::new(self.postgres.clone());
        let response = controller.get_pnl_leaderboard(query).await.map_err(|err| {
            AppError::InternalError(format!("Failed to get pnl leaderboard: {}", err))
        })?;

        if let Err(err) = self
            .redis
            .set_pnl_leaderboard_response(query.page, query.limit, &response)
            .await
        {
            error!("Failed to set pnl leaderboard response: {}", err);
        }

        Ok(response)
    }
}
