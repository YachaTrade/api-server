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
        let mut tx = self.db.pool.begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO account_like (liker_id, liking_id)
            VALUES ($1, $2)
            "#,
            liker_id,
            liking_id,
        )
        .execute(tx.as_mut())
        .await
        .map_err(|err| anyhow!("Insert Account like\n Reason : {:?}", err))?;

        let liker_account = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET like_count = like_count + 1
            WHERE account_id = $1
            RETURNING *
            "#,
            liker_id
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|err| anyhow!("Update Account like_count\n Reason : {:?}", err))?;

        tx.commit().await?;

        debug!(
            "Accout like inserted Success {:?}:{:?}",
            liker_id, liking_id
        );
        Ok(liker_account)
    }

    pub async fn remove_account_like(&self, liker_id: &str, liking_id: &str) -> Result<Account> {
        debug!("Removing Account like {:?}:{:?}", liker_id, liking_id);
        let mut tx = self.db.pool.begin().await?;

        sqlx::query!(
            r#"
            DELETE FROM account_like
            WHERE liker_id = $1 AND liking_id = $2
            "#,
            liker_id,
            liking_id
        )
        .execute(tx.as_mut())
        .await
        .map_err(|err| anyhow!("DELETE account_like \n Reason : {:?}", err))?;

        let liker_account = sqlx::query_as!(
            Account,
            r#"
            UPDATE account
            SET like_count = GREATEST(like_count - 1, 0)
            WHERE account_id = $1
            RETURNING *
            "#,
            liker_id
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|err| anyhow!("Update Account like_count\n Reason : {:?}", err))?;

        tx.commit().await?;

        debug!(
            "Account like removed successfully. Liker ID: {}, Liking ID: {}",
            liker_id, liking_id
        );
        Ok(liker_account)
    }
}
