use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{ExistsRow, info::AccountInfo, pagination::PaginationParams},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFollowRequest {
    pub follower: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpdateFollowResponse {
    pub follower: AccountInfo,
    pub following: AccountInfo,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct FollowsResponse {
    pub accounts: Vec<Follow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowResponse {
    follower: AccountInfo,
    following: AccountInfo,
}

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Follow {
    pub account: AccountInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckFollowResponse {
    pub is_following: bool,
}

pub struct FollowController {
    db: Arc<PostgresDatabase>,
}

impl FollowController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        FollowController { db }
    }

    /// Get followers or following accounts
    /// * `account_id` - The account ID to get followers/following for
    /// * `is_following` - If true, get accounts that account_id is following
    ///                   If false, get accounts that are following account_id
    pub async fn get_follows(
        &self,
        account_id: &str,
        is_following: bool,
        pagination: PaginationParams,
    ) -> Result<Vec<Follow>> {
        let offset = (pagination.page - 1) * pagination.limit;

        let follows = measure_postgres!(
            "follow.get_follows",
            sqlx::query_as::<_, AccountInfo>(
                r#"
                SELECT 
                    a.account_id,
                    a.nickname,
                    a.image_uri,
                    a.follower_count,
                    a.following_count
                FROM follow f
                JOIN account a ON CASE 
                    WHEN $2 = true THEN a.account_id = f.following_id
                    ELSE a.account_id = f.follower_id
                END
                WHERE CASE 
                    WHEN $2 = true THEN f.follower_id = $1
                    ELSE f.following_id = $1
                END
                ORDER BY a.follower_count DESC
                LIMIT $3
                OFFSET $4
                "#,
            )
            .bind(account_id)
            .bind(is_following)
            .bind(pagination.limit as i64)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch follows: {}", err))?;

        let follows = follows
            .into_iter()
            .map(|account| Follow { account })
            .collect();

        Ok(follows)
    }

    pub async fn add_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        #[derive(sqlx::FromRow)]
        struct FollowResult {
            follower_info: serde_json::Value,
            following_info: serde_json::Value,
        }

        let result = measure_postgres!(
            "follow.add_follow",
            sqlx::query_as::<_, FollowResult>(
                r#"
                WITH follow_insert AS (
                    INSERT INTO follow (follower_id, following_id)
                    VALUES ($1, $2)
                    ON CONFLICT DO NOTHING
                    RETURNING follower_id, following_id
                ),
                follower_update AS (
                    UPDATE account
                    SET follower_count = follower_count + 1
                    WHERE account_id = $1
                    AND EXISTS (SELECT 1 FROM follow_insert)
                    RETURNING account_id, nickname, image_uri, follower_count, following_count
                ),
                following_update AS (
                    UPDATE account
                    SET following_count = following_count + 1
                    WHERE account_id = $2
                    AND EXISTS (SELECT 1 FROM follow_insert)
                    RETURNING account_id, nickname, image_uri, follower_count, following_count
                )
                SELECT 
                    (SELECT row_to_json(follower_update.*) FROM follower_update) as follower_info,
                    (SELECT row_to_json(following_update.*) FROM following_update) as following_info
                WHERE EXISTS (SELECT 1 FROM follow_insert)
                "#,
            )
            .bind(&follower)
            .bind(&following)
            .fetch_optional(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to add follow: {}", err))?;

        let result = result.ok_or_else(|| anyhow!("Follow already exists or failed"))?;
        let follower_info: AccountInfo = serde_json::from_value(result.follower_info)?;
        let following_info: AccountInfo = serde_json::from_value(result.following_info)?;

        Ok((follower_info, following_info))
    }

    pub async fn remove_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        #[derive(sqlx::FromRow)]
        struct FollowResult {
            follower_info: serde_json::Value,
            following_info: serde_json::Value,
        }

        let result = measure_postgres!(
            "follow.remove_follow",
            sqlx::query_as::<_, FollowResult>(
                r#"
                WITH follow_delete AS (
                    DELETE FROM follow 
                    WHERE follower_id = $1 AND following_id = $2
                    RETURNING follower_id, following_id
                ),
                follower_update AS (
                    UPDATE account
                    SET follower_count = GREATEST(follower_count - 1, 0)
                    WHERE account_id = $1
                    AND EXISTS (SELECT 1 FROM follow_delete)
                    RETURNING account_id, nickname, image_uri, follower_count, following_count
                ),
                following_update AS (
                    UPDATE account
                    SET following_count = GREATEST(following_count - 1, 0)
                    WHERE account_id = $2
                    AND EXISTS (SELECT 1 FROM follow_delete)
                    RETURNING account_id, nickname, image_uri, follower_count, following_count
                )
                SELECT 
                    (SELECT row_to_json(follower_update.*) FROM follower_update) as follower_info,
                    (SELECT row_to_json(following_update.*) FROM following_update) as following_info
                WHERE EXISTS (SELECT 1 FROM follow_delete)
                "#,
            )
            .bind(&follower)
            .bind(&following)
            .fetch_optional(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to remove follow: {}", err))?;

        let result = result.ok_or_else(|| anyhow!("Follow relationship not found or failed"))?;
        let follower_info: AccountInfo = serde_json::from_value(result.follower_info)?;
        let following_info: AccountInfo = serde_json::from_value(result.following_info)?;

        Ok((follower_info, following_info))
    }

    pub async fn check_follow(&self, follower: String, following: String) -> Result<bool> {
        let result = measure_postgres!(
            "follow.check_follow",
            sqlx::query_as::<_, ExistsRow>(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM follow 
                    WHERE follower_id = $1 AND following_id = $2
                ) as exists
                "#,
            )
            .bind(&follower)
            .bind(&following)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check follow: {}", err))?;

        Ok(result.exists)
    }

    async fn insert_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        measure_postgres!(
            "follow.insert_follow",
            sqlx::query(
                r#"
                INSERT INTO follow (follower_id, following_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(follower)
            .bind(following)
            .execute(tx)
        )
        .map_err(|err| anyhow!("Failed to insert follow\n Reason :{err}"))?;

        Ok(())
    }

    async fn delete_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        measure_postgres!(
            "follow.delete_follow",
            sqlx::query(
                r#"
                DELETE FROM follow
                WHERE follower_id = $1 AND following_id = $2
                "#,
            )
            .bind(follower)
            .bind(following)
            .execute(tx.as_mut())
        )
        .map_err(|err| anyhow!("Failed delete follow\n Reason:{} ", err))?;
        Ok(())
    }
}
