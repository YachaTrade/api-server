use std::sync::Arc;

use crate::db::postgres::{model::Account, PostgresDatabase};

use anyhow::{anyhow, Result};
use sqlx::{Postgres, QueryBuilder};

pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn upsert_account(&self, account: Account) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count, like_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (account_id) 
            DO UPDATE SET
                image_uri = $2,
                nickname = $3,
                bio = $4,
                follower_count = $5,
                following_count = $6,
                like_count = $7
                RETURNING *
            "#,
            account.account_id,
            account.image_uri,
            account.nickname,
            account.bio,
            account.follower_count,
            account.following_count,
            account.like_count,
        )
        .fetch_one(self.db.get_write_pool())
        .await
        .map_err(|err| anyhow!("Fail upsert account Reason :{:?}", err))?;
        Ok(account)
    }
    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<Account> {
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE account SET ");
        let mut changed = false;

        if let Some(image_uri) = image_uri {
            if !changed {
                query_builder.push(" image_uri = ");
            } else {
                query_builder.push(" , image_uri = ");
            }
            query_builder.push_bind(image_uri);
            changed = true;
        }

        if let Some(nickname) = nickname {
            if !changed {
                query_builder.push(" nickname = ");
            } else {
                query_builder.push(" , nickname = ");
            }
            query_builder.push_bind(nickname);
            changed = true;
        }

        if let Some(bio) = bio {
            if !changed {
                query_builder.push(" bio = ");
            } else {
                query_builder.push(" , bio = ");
            }
            query_builder.push_bind(bio);
            changed = true;
        }

        query_builder.push(" WHERE account_id = ");
        query_builder.push_bind(address);

        let query = query_builder.build();
        query.execute(self.db.get_write_pool()).await?;

        // Get updated account
        let updated_account = sqlx::query_as!(
            Account,
            r#"
            SELECT 
                account_id,
                image_uri, 
                nickname,
                bio,
                follower_count,
                following_count,
                like_count
            FROM account
            WHERE LOWER(account_id) = LOWER($1)
            "#,
            address
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow!("Fail update account Reason :{err} address: {}", err))?;

        Ok(updated_account)
    }
    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            SELECT *
            FROM account
            WHERE LOWER(account_id) = LOWER($1)
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow!("Fail get account Reason :{err} address: {}", err))?;
        Ok(account)
    }
}
