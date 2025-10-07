use std::sync::Arc;

use crate::{
    controllers::social::FollowController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::{
        common::{info::AccountInfo, pagination::PaginationParams},
        social::follow::{
            FollowersResponse, FollowingResponse, UpdateFollowResponse,
        },
    },
};

pub struct SocialService {
    postgres: Arc<PostgresDatabase>,
}

impl SocialService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn add_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<UpdateFollowResponse, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        controller
            .add_follow(follower, following)
            .await
            .map(|(follower, following)| UpdateFollowResponse {
                follower,
                following,
            })
            .map_err(|err| AppError::BadRequest(err.to_string()))
    }

    pub async fn remove_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<UpdateFollowResponse, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        controller
            .remove_follow(follower, following)
            .await
            .map(|(follower, following)| UpdateFollowResponse {
                follower,
                following,
            })
            .map_err(|err| AppError::BadRequest(err.to_string()))
    }

    pub async fn check_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<bool, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        controller
            .check_follow(follower, following)
            .await
            .map_err(|err| AppError::BadRequest(err.to_string()))
    }

    pub async fn get_followers(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<FollowersResponse, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        let (accounts, total_count) = controller
            .get_follows(account_id, false, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        Ok(FollowersResponse {
            accounts,
            total_count,
        })
    }

    pub async fn get_following(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<FollowingResponse, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        let (accounts, total_count) = controller
            .get_follows(account_id, true, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        Ok(FollowingResponse {
            accounts,
            total_count,
        })
    }
}
