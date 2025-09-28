pub mod wallet;
pub mod x;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};
use tracing::warn;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::account::Account,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(sqlx::FromRow)]
struct AccountRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    bio: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
}

#[derive(sqlx::FromRow)]
struct AccountMutualRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    bio: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
    mutual_friends: Option<serde_json::Value>,
    mutual_friends_count: Option<i64>,
}

pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn upsert_account(&self, account: Account) -> Result<Account> {
        let start_time = Instant::now();

        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (account_id)
            DO NOTHING
            RETURNING account_id, nickname, image_uri, bio, follower_count, following_count, NULL as x_handle, NULL as x_image_uri, NULL as is_blue_label
            "#,
        )
        .bind(&account.account_id)
        .bind(&account.image_uri)
        .bind(&account.nickname)
        .bind(&account.bio)
        .bind(account.follower_count)
        .bind(account.following_count)
        .fetch_optional(self.db.get_write_pool());

        let result = measure_postgres!("account.upsert_account", query)
            .map_err(|err| anyhow!("Failed to upsert account. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        if elapsed > Duration::from_millis(100) {
            warn!(
                "upsert_account query slow performance: {:?} for account_id: {}",
                elapsed, account.account_id
            );
        }

        match result {
            Some(row) => {
                let account = Account {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                    bio: row.bio,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                    mutual: None,
                };
                Ok(account)
            }
            None => {
                let query = sqlx::query_as::<_, AccountRow>(
                    r#"
                   SELECT a.account_id,
                        a.nickname,
                        a.image_uri,
                        a.bio,
                        a.follower_count,
                        a.following_count,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label
                        FROM account a
                        LEFT JOIN account_x ax ON a.account_id = ax.account_id
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    WHERE a.account_id = $1
                    "#,
                )
                .bind(&account.account_id)
                .fetch_optional(self.db.get_read_pool());
                let row = measure_postgres!("account.fetch_existing", query)
                    .map_err(|err| anyhow!("Query timeout or error occurred. Reason: {:?}", err))?;
                let row = match row {
                    Some(row) => row,
                    None => {
                        return Err(anyhow!("Account not found: {}", account.account_id));
                    }
                };
                let account = Account {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                    bio: row.bio,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                    mutual: None,
                };
                Ok(account)
            }
        }
    }

    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<Account> {
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE account SET ");

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
            .push(" RETURNING *");

        let query = query_builder.build();
        let update_query = query
            .try_map(|row: PgRow| {
                Ok(Account {
                    account_id: row.try_get("account_id")?,
                    image_uri: row.try_get("image_uri")?,
                    nickname: row.try_get("nickname")?,
                    bio: row.try_get("bio")?,
                    follower_count: row.try_get("follower_count")?,
                    following_count: row.try_get("following_count")?,
                    mutual: None,
                })
            })
            .fetch_one(self.db.get_write_pool());

        let updated_account = measure_postgres!("account.update_account", update_query)
            .map_err(|err| anyhow!("Fail update account. Reason: {err} address: {}", address))?;

        Ok(updated_account)
    }

    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let cache_key = cache_key!("account", account_id);

        let account = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_account(account_id).await
        })
        .await?;

        Ok(account)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Account> {
        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT a.account_id,
            a.nickname,
            a.image_uri,
            a.bio,
            a.follower_count,
            a.following_count,
            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
            ax.x_image_uri,
            ax.is_blue_label
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE a.account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(self.db.get_read_pool());

        let row = measure_postgres!("account.fetch_account", query)
            .map_err(|err| anyhow!("Query timeout or error occurred. Reason: {:?}", err))?;

        let row = match row {
            Some(row) => row,
            None => {
                let account = Account::new(account_id.to_string());
                self.upsert_account(account.clone())
                    .await
                    .map_err(|err| anyhow!("Fail upsert account Reason :{err} address: {}", err))?;
                return Ok(account);
            }
        };
        let account = Account {
            account_id: row.account_id,
            nickname: match &row.x_handle {
                Some(handle) if !handle.is_empty() => handle.clone(),
                _ => row.nickname,
            },
            image_uri: match &row.x_image_uri {
                Some(img) if !img.is_empty() => img.clone(),
                _ => row.image_uri,
            },
            bio: row.bio,
            follower_count: row.follower_count,
            following_count: row.following_count,
            mutual: None,
        };

        Ok(account)
    }
}
