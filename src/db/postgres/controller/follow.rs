use std::sync::Arc;

use crate::db::postgres::{model::Account, PostgresDatabase};
use anyhow::{anyhow, Result};

pub struct FollowController {
    db: Arc<PostgresDatabase>,
}

impl FollowController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        FollowController { db }
    }

    pub async fn add_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(Account, Account)> {
        let mut tx = self.db.write_pool.begin().await?;

        self.insert_follow(&mut tx, &follower, &following).await?;
        // let follower_account = self.update_follower_count(&mut tx, &follower, 1).await?;
        let follower = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET follower_count = follower_count +1
            WHERE account_id = $1
            RETURNING *
            "#,
            follower,
        )
        .fetch_one(tx.as_mut())
        .await?;

        let following = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET following_count =following_count +1
            WHERE account_id = $1
            RETURNING *
            "#,
            following,
        )
        .fetch_one(tx.as_mut())
        .await?;

        tx.commit().await?;
        Ok((follower, following))
    }

    pub async fn remove_follow(
        &self,
        follower: String,
        following: String,
    ) -> Result<(Account, Account)> {
        let mut tx = self.db.write_pool.begin().await?;

        self.delete_follow(&mut tx, &follower, &following).await?;

        let follower = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET follower_count = GREATEST(follower_count -1,0)
            WHERE account_id = $1
            RETURNING *
            "#,
            follower
        )
        .fetch_one(tx.as_mut())
        .await?;

        let follwing = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET following_count =  GREATEST(follower_count - 1, 0)
            WHERE account_id = $1
            RETURNING *
            "#,
            following
        )
        .fetch_one(tx.as_mut())
        .await?;

        // let follower_account = self.update_follower_count(&mut tx, &follower, -1).await?;
        // let following_account = self.update_following_count(&mut tx, &following, -1).await?;

        tx.commit().await?;
        Ok((follower, follwing))
    }

    async fn insert_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO follow (follower_id, following_id)
            VALUES ($1, $2)
            "#,
            follower,
            following
        )
        .execute(tx)
        .await
        .map_err(|err| anyhow!("Failed to insert follow\n Reason :{err}"))?;

        Ok(())
    }

    async fn delete_follow(
        &self,
        tx: &mut sqlx::PgConnection,
        follower: &str,
        following: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM follow
            WHERE follower_id = $1 AND following_id = $2
            "#,
            follower,
            following
        )
        .execute(&mut *tx)
        .await
        .map_err(|err| anyhow!("Failed delete follow\n Reason:{} ", err))?;
        Ok(())
    }

    pub async fn get_followers(
        &self,
        user_id: String,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Account>> {
        let followers = sqlx::query_as!(
            Account,
            r#"
            SELECT a.account_id, a.nickname,a.bio, a.image_uri, a.follower_count, a.following_count,a.like_count
            FROM follow f
            JOIN account a ON f.follower_id = a.account_id
            WHERE f.following_id = $1
            ORDER BY a.account_id
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            limit,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?;

        Ok(followers)
    }

    pub async fn get_following(
        &self,
        user_id: String,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Account>> {
        let following = sqlx::query_as!(
            Account,
            r#"
            SELECT a.account_id, a.nickname,a.bio, a.image_uri, a.follower_count, a.following_count,a.like_count
            FROM follow f
            JOIN account a ON f.following_id = a.account_id
            WHERE f.follower_id = $1
            ORDER BY a.account_id
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            limit,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?;

        Ok(following)
    }
}
