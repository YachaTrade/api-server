use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{db::postgres::PostgresDatabase, measure_postgres, types::account::Account};

#[derive(Debug, FromRow)]
struct SessionRow {
    account_id: String,
}

#[derive(FromRow)]
pub struct AccountRow {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub bio: String,
    pub follower_count: i32,
    pub following_count: i32,
}

pub struct SessionController {
    pub db: Arc<PostgresDatabase>,
}

impl SessionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SessionController { db }
    }

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<AccountRow> {
        let account = Account::new(address.to_string());
        let account_info = measure_postgres!(
            "auth.set_session",
            sqlx::query_as::<_, AccountRow>(
                r#"
                WITH account_upsert AS (
                    INSERT INTO account (account_id, nickname, image_uri, bio, follower_count, following_count)
                    VALUES ($2, $3, $4, $5, $6, $7)
                    ON CONFLICT (account_id) DO NOTHING
                    RETURNING *
                ),
                session_upsert AS (
                    INSERT INTO account_session (id, account_id)
                    VALUES ($1, $2)
                    ON CONFLICT (account_id) DO UPDATE
                    SET id = EXCLUDED.id
                    RETURNING account_id
                )
                SELECT a.account_id, a.nickname, a.image_uri, a.bio, a.follower_count, a.following_count
                FROM account a 
                WHERE a.account_id = $2
                "#,
            )
            .bind(session_id)
            .bind(address)
            .bind(&account.nickname)
            .bind(&account.image_uri)
            .bind(&account.bio)
            .bind(account.follower_count)
            .bind(account.following_count)
            .fetch_one(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set session: {}", err))?;

        Ok(account_info)
    }
    pub async fn get_address_by_session_id(&self, session_id: &str) -> Result<String> {
        let session = measure_postgres!(
            "auth.get_address_by_session_id",
            sqlx::query_as::<_, SessionRow>(
                r#"
                SELECT account_id FROM account_session WHERE id = $1
                "#,
            )
            .bind(session_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get session: {}", err))?;

        Ok(session.account_id)
    }

    pub async fn delete_session_by_id(&self, session_id: &str) -> Result<()> {
        measure_postgres!(
            "auth.delete_session_by_id",
            sqlx::query(
                r#"
                DELETE FROM account_session WHERE id = $1
                "#,
            )
            .bind(session_id)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to delete session: {}", err))?;

        Ok(())
    }
}
