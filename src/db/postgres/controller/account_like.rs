use std::sync::Arc;

use crate::db::postgres::{model::Account, PostgresDatabase};

use anyhow::{anyhow, Result};

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
        .map_err(|err| anyhow!("Fail insert Account like Reason :{:?}", err))?;

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
        .map_err(|err| anyhow!("Fail update Account like Reason :{:?}", err))?;

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
        .map_err(|err| anyhow!("Fail Update like Account Reason :{:?}", err))?;

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
        .map_err(|err| anyhow!("Fail delete Account like Reason :{:?}", err))?;

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
        .map_err(|err| anyhow!("Fail update Account like Reason :{:?}", err))?;

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
        .map_err(|err| anyhow!("Fail get Account Reason :{:?}", err))?;

        Ok(liker_account)
    }
}
