use std::sync::Arc;

use crate::db::postgres::{model::Account, PostgresDatabase};

use anyhow::{anyhow, Context, Result};

use tracing::debug;

pub struct AccountLikeController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountLikeController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountLikeController { db }
    }

    pub async fn add_account_like(&self, liker_id: &str, liking_id: &str) -> Result<Account> {
        debug!("Inserting Account like {:?}:{:?}", liker_id, liking_id);
        let mut tx = self.db.get_write_pool().begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO account_like (liker_id, liking_id)
            VALUES ($1, $2)
            "#,
            liker_id,
            liking_id
        )
        .execute(&mut *tx)
        .await
        .context("Fail insert Account like")?;

        sqlx::query!(
            r#"
            UPDATE account
            SET like_count = like_count + 1
            WHERE account_id = $1
            "#,
            liking_id
        )
        .execute(&mut *tx)
        .await
        .context("Fail update Account like")?;

        tx.commit().await?;

        let liker_account = sqlx::query_as!(
            Account,
            r#"
            SELECT account_id,image_uri,nickname,bio,follower_count,following_count,like_count
            FROM account
            WHERE account_id = $1
            "#,
            liking_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Fail get Account")?;

        Ok(liker_account)
    }

    pub async fn remove_account_like(&self, liker_id: &str, liking_id: &str) -> Result<Account> {
        debug!("Delete Account like {:?}:{:?}", liker_id, liking_id);
        let mut tx = self.db.get_write_pool().begin().await?;
        sqlx::query!(
            r#"
            DELETE FROM account_like
            WHERE liker_id = $1 AND liking_id = $2
            "#,
            liker_id,
            liking_id
        )
        .execute(&mut *tx)
        .await
        .context("Fail delete Account like")?;

        sqlx::query!(
            r#"
            UPDATE account
            SET like_count = like_count - 1
            WHERE account_id = $1
            "#,
            liking_id
        )
        .execute(&mut *tx)
        .await
        .context("Fail update Account like")?;

        tx.commit().await?;

        let liker_account = sqlx::query_as!(
            Account,
            r#"
            SELECT account_id,image_uri,nickname,bio,follower_count,following_count,like_count
            FROM account
            WHERE account_id = $1
            "#,
            liking_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Fail get Account")?;

        Ok(liker_account)
    }
}
