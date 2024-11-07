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
        .execute(&self.db.pool)
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
    
        let mut updates = Vec::new();
    
        if let Some(image_uri) = image_uri {
            updates.push(("image_uri", image_uri));
        }
        if let Some(nickname) = nickname {
            updates.push(("nickname", nickname));
        }
        if let Some(bio) = bio {
            updates.push(("bio", bio));
        }
    
        for (i, (field, value)) in updates.iter().enumerate() {
            if i > 0 {
                query_builder.push(", ");
            }
            query_builder.push(format!(" {} = ", field));
            query_builder.push_bind(value);
        }
    
        query_builder.push(" WHERE id = ");
        query_builder.push_bind(address);
    
        query_builder.push(" RETURNING *");
    
        let updated_account = query_builder
            .build_query_as::<Account>()
            .fetch_one(&self.db.pool)
            .await?;
    
        Ok(updated_account)
    }
    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            SELECT * FROM account WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_one(&self.db.pool)
        .await?;

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
