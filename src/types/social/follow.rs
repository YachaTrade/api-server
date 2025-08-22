use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{ExistsRow, info::AccountInfo, pagination::PaginationParams},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
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
        let start_time = Instant::now();
        let offset = (pagination.page - 1) * pagination.limit;

        let follows = tokio::time::timeout(
            Duration::from_millis(1000),
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
                    WHEN $2 = true THEN a.account_id = f.following_id  -- Get following
                    ELSE a.account_id = f.follower_id                  -- Get followers
                END
                WHERE CASE 
                    WHEN $2 = true THEN f.follower_id = $1   -- account_id is following others
                    ELSE f.following_id = $1                 -- others are following account_id
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
            .fetch_all(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let follows = follows
            .into_iter()
            .map(|account| Follow { account })
            .collect();

        let elapsed = start_time.elapsed();
        info!("get_follows(account_id: {}, is_following: {}) completed in {:?}", account_id, is_following, elapsed);
        Ok(follows)
    }

    pub async fn add_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        let start_time = Instant::now();
        let mut tx = self.db.get_write_pool().begin().await?;

        self.insert_follow(&mut tx, &follower, &following).await?;

        let follower = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, AccountInfo>(
                r#"
                UPDATE account
                SET following_count = following_count + 1
                WHERE account_id = $1
                RETURNING account_id, nickname, image_uri, follower_count, following_count
                "#,
            )
            .bind(&follower)
            .fetch_one(tx.as_mut()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let following = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, AccountInfo>(
                r#"
                UPDATE account
                SET follower_count = follower_count + 1
                WHERE account_id = $1
                RETURNING account_id, nickname, image_uri, follower_count, following_count
                "#,
            )
            .bind(&following)
            .fetch_one(tx.as_mut()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        tx.commit().await?;
        let elapsed = start_time.elapsed();
        info!("add_follow(follower: {}, following: {}) completed in {:?}", follower.account_id, following.account_id, elapsed);
        Ok((follower, following))
    }

    pub async fn remove_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        let start_time = Instant::now();
        let mut tx = self.db.get_write_pool().begin().await?;

        self.delete_follow(&mut tx, &follower, &following).await?;

        let follower = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, AccountInfo>(
                r#"
                UPDATE account
                SET following_count = GREATEST(following_count - 1, 0)
                WHERE account_id = $1
                RETURNING account_id, nickname, image_uri, follower_count, following_count
                "#,
            )
            .bind(&follower)
            .fetch_one(tx.as_mut()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let following = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, AccountInfo>(
                r#"
                UPDATE account
                SET follower_count = GREATEST(follower_count - 1, 0)
                WHERE account_id = $1
                RETURNING account_id, nickname, image_uri, follower_count, following_count
                "#,
            )
            .bind(&following)
            .fetch_one(tx.as_mut()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))?
        .map_err(|err| anyhow!("Failed to remove follow\n Reason :{err}"))?;

        tx.commit().await?;
        let elapsed = start_time.elapsed();
        info!("remove_follow(follower: {}, following: {}) completed in {:?}", follower.account_id, following.account_id, elapsed);
        Ok((follower, following))
    }

    pub async fn check_follow(&self, follower: String, following: String) -> Result<bool> {
        let start_time = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_millis(1000),
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
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let elapsed = start_time.elapsed();
        info!("check_follow(follower: {}, following: {}) completed in {:?}", follower, following, elapsed);
        Ok(result.exists)
    }

    async fn insert_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query(
                r#"
                INSERT INTO follow (follower_id, following_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(follower)
            .bind(following)
            .execute(tx),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))?
        .map_err(|err| anyhow!("Failed to insert follow\n Reason :{err}"))?;

        Ok(())
    }

    async fn delete_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query(
                r#"
                DELETE FROM follow
                WHERE follower_id = $1 AND following_id = $2
                "#,
            )
            .bind(follower)
            .bind(following)
            .execute(tx.as_mut()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))?
        .map_err(|err| anyhow!("Failed delete follow\n Reason:{} ", err))?;
        Ok(())
    }
}
