use std::sync::Arc;

use crate::{
    db::postgres::{model::Account, PostgresDatabase},
    env,
};

use anyhow::{Context, Result};
use sqlx::{Postgres, QueryBuilder};
use tracing::debug;

pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn insert_account(&self, account: Account) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO account (account_id, image_uri,nickname,bio, follower_count, following_count,like_count)
            VALUES ($1, $2, $3, $4,$5,$6,$7)
            "#,
            account.account_id,
            account.image_uri,
            account.nickname,
            account.bio,
            account.follower_count,
            account.following_count,
            account.like_count,
         
        )
        .execute(self.db.get_write_pool())
        .await
        .context("Fail insert Account")?;
        Ok(())
    }
    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<Account> {
        debug!("bio : {:?}", bio);
        debug!("nickname : {:?}", nickname);
        debug!("image_uri : {:?}", image_uri);
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

        let  query = query_builder.build();
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
            WHERE account_id = $1
            "#,
            address
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(updated_account)
    }
    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            SELECT account_id,image_uri,nickname,bio,follower_count,following_count,like_count
            FROM account
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Fail get Account")?;
        Ok(account)
    }
    pub async fn get_or_create_account(&self, address: &str) -> Result<Account> {
        match self.get_account(address).await {
            Ok(account) => Ok(account),
            Err(_) => {
                let account = Account::new(address.to_string());
                self.insert_account(account.clone()).await?;
                Ok(account)
            }
        }
    }
}
