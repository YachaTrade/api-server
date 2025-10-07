use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{CountRow, ExistsRow, info::AccountInfo, pagination::PaginationParams},
};

pub struct FollowController {
    db: Arc<PostgresDatabase>,
}

impl FollowController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        FollowController { db }
    }

    pub async fn get_follows(
        &self,
        account_id: &str,
        is_following: bool,
        pagination: PaginationParams,
    ) -> Result<(Vec<AccountInfo>, i64)> {
        let offset = (pagination.page - 1) * pagination.limit;

        let total_count = measure_postgres!(
            "follow.get_follows_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(*) as total_count
                FROM follow f
                WHERE CASE
                    WHEN $2 = true THEN f.follower_id = $1
                    ELSE f.following_id = $1
                END
                "#,
            )
            .bind(account_id)
            .bind(is_following)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch follow count: {}", err))?
        .count;

        let accounts = measure_postgres!(
            "follow.get_follows",
            sqlx::query_as::<_, AccountInfo>(
                r#"
                SELECT
                    a.account_id,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    a.bio,
                    a.follower_count,
                    a.following_count
                FROM follow f
                JOIN account a ON CASE
                    WHEN $2 = true THEN a.account_id = f.following_id
                    ELSE a.account_id = f.follower_id
                END
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
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

        Ok((accounts, total_count))
    }

    pub async fn add_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        let accounts = measure_postgres!(
            "follow.add_follow",
            sqlx::query_as::<_, AccountInfo>(
                r#"
                WITH follow_insert AS (
                    INSERT INTO follow (follower_id, following_id)
                    VALUES ($1, $2)
                    ON CONFLICT DO NOTHING
                    RETURNING follower_id, following_id
                ),
                account_update AS (
                    UPDATE account
                    SET following_count = following_count + CASE WHEN account_id = $1 THEN 1 ELSE 0 END,
                        follower_count = follower_count + CASE WHEN account_id = $2 THEN 1 ELSE 0 END
                    WHERE (account_id = $1 OR account_id = $2)
                    AND EXISTS (SELECT 1 FROM follow_insert)
                    RETURNING account_id
                )
                SELECT
                    a.account_id,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    a.bio,
                    a.follower_count,
                    a.following_count
                FROM account a
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                WHERE a.account_id IN ($1, $2)
                AND EXISTS (SELECT 1 FROM account_update)
                ORDER BY CASE WHEN a.account_id = $1 THEN 0 ELSE 1 END
                "#,
            )
            .bind(&follower)
            .bind(&following)
            .fetch_all(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to add follow: {}", err))?;

        if accounts.len() != 2 {
            return Err(anyhow!("Failed to fetch both accounts"));
        }

        Ok((accounts[0].clone(), accounts[1].clone()))
    }

    pub async fn remove_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(AccountInfo, AccountInfo)> {
        let accounts = measure_postgres!(
            "follow.remove_follow",
            sqlx::query_as::<_, AccountInfo>(
                r#"
                WITH follow_delete AS (
                    DELETE FROM follow
                    WHERE follower_id = $1 AND following_id = $2
                    RETURNING follower_id, following_id
                ),
                account_update AS (
                    UPDATE account
                    SET following_count = GREATEST(following_count - CASE WHEN account_id = $1 THEN 1 ELSE 0 END, 0),
                        follower_count = GREATEST(follower_count - CASE WHEN account_id = $2 THEN 1 ELSE 0 END, 0)
                    WHERE (account_id = $1 OR account_id = $2)
                    AND EXISTS (SELECT 1 FROM follow_delete)
                    RETURNING account_id
                )
                SELECT
                    a.account_id,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    a.bio,
                    a.follower_count,
                    a.following_count
                FROM account a
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                WHERE a.account_id IN ($1, $2)
                AND EXISTS (SELECT 1 FROM account_update)
                ORDER BY CASE WHEN a.account_id = $1 THEN 0 ELSE 1 END
                "#,
            )
            .bind(&follower)
            .bind(&following)
            .fetch_all(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to remove follow: {}", err))?;

        if accounts.len() != 2 {
            return Err(anyhow!("Failed to fetch both accounts"));
        }

        Ok((accounts[0].clone(), accounts[1].clone()))
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
}
