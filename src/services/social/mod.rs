use std::sync::Arc;

use crate::{
    controllers::social::FollowController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        social::follow::{Follow, UpdateFollowResponse},
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

    pub async fn get_follows(
        &self,
        account_id: &str,
        is_following: bool,
        pagination: PaginationParams,
    ) -> Result<Vec<Follow>, AppError> {
        let controller = FollowController::new(self.postgres.clone());
        controller
            .get_follows(account_id, is_following, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))
    }
}
