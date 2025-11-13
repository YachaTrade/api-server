pub mod wallet;
pub mod x;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use sqlx::{Postgres, QueryBuilder};
use tracing::warn;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::info::AccountInfo,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn upsert_account(&self, account: AccountInfo) -> Result<AccountInfo> {
        let start_time = Instant::now();

        let query = sqlx::query_as::<_, AccountInfo>(
            r#"
            WITH upsert AS (
                INSERT INTO account (account_id, image_uri, nickname, bio)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (account_id)
                DO NOTHING
                RETURNING account_id
            )
            SELECT
                a.account_id,
                COALESCE(ax.x_handle, a.nickname) as nickname,
                COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                a.bio
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            WHERE a.account_id = $1
            "#,
        )
        .bind(&account.account_id)
        .bind(&account.image_uri)
        .bind(&account.nickname)
        .bind(&account.bio)
        .fetch_one(self.db.get_write_pool());

        let account = measure_postgres!("account.upsert_account", query)
            .map_err(|err| anyhow!("Failed to upsert account. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        if elapsed > Duration::from_millis(100) {
            warn!(
                "upsert_account query slow performance: {:?} for account_id: {}",
                elapsed, account.account_id
            );
        }

        Ok(account)
    }

    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<AccountInfo> {
        let mut query_builder: QueryBuilder<Postgres> =
            QueryBuilder::new("WITH updated AS (UPDATE account SET ");

        let mut fields = vec![];

        if let Some(image_uri) = image_uri {
            fields.push(("image_uri", image_uri));
        }

        if let Some(nickname) = nickname {
            fields.push(("nickname", nickname));
        }

        if let Some(bio) = bio {
            fields.push(("bio", bio));
        }

        if fields.is_empty() {
            return Err(anyhow!("No fields provided to update"));
        }

        for (i, (field_name, field_value)) in fields.into_iter().enumerate() {
            if i > 0 {
                query_builder.push(", ");
            }
            query_builder.push(format!("{} = ", field_name));
            query_builder.push_bind(field_value);
        }

        query_builder
            .push(" WHERE account_id = ")
            .push_bind(address)
            .push(" RETURNING account_id) SELECT a.account_id, COALESCE(ax.x_handle, a.nickname) as nickname, COALESCE(ax.x_image_uri, a.image_uri) as image_uri, a.bio FROM account a LEFT JOIN account_x ax ON a.account_id = ax.account_id WHERE a.account_id = ")
            .push_bind(address);

        let query = query_builder
            .build_query_as::<AccountInfo>()
            .fetch_one(self.db.get_write_pool());

        let account = measure_postgres!("account.update_account", query)
            .map_err(|err| anyhow!("Fail update account. Reason: {err} address: {}", address))?;

        Ok(account)
    }

    pub async fn get_account(&self, account_id: &str) -> Result<AccountInfo> {
        let cache_key = cache_key!("account", account_id);

        let account = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_account(account_id).await
        })
        .await?;

        Ok(account)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<AccountInfo> {
        let query = sqlx::query_as::<_, AccountInfo>(
            r#"
            SELECT
                a.account_id,
                COALESCE(ax.x_handle, a.nickname) as nickname,
                COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                a.bio
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            WHERE a.account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(self.db.get_read_pool());

        let account = measure_postgres!("account.fetch_account", query)
            .map_err(|err| anyhow!("Query timeout or error occurred. Reason: {:?}", err))?;

        match account {
            Some(account) => Ok(account),
            None => {
                // Create and insert new account
                let new_account = AccountInfo::new(account_id.to_string());
                self.upsert_account(new_account).await
            }
        }
    }
}
