use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{db::postgres::PostgresDatabase, measure_postgres, types::common::info::AccountInfo};

#[derive(Debug, FromRow)]
struct SessionRow {
    account_id: String,
}

pub struct SessionController {
    pub db: Arc<PostgresDatabase>,
}

impl SessionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SessionController { db }
    }

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<AccountInfo> {
        let account = AccountInfo::new(address.to_string());
        let account_info = measure_postgres!(
            "auth.set_session",
            sqlx::query_as::<_, AccountInfo>(
                r#"
                WITH account_upsert AS (
                    INSERT INTO account (account_id, nickname, image_uri, bio)
                    VALUES ($2, $3, $4, $5)
                    ON CONFLICT (account_id) DO UPDATE
                    SET account_id = EXCLUDED.account_id
                    RETURNING *
                ),
                session_upsert AS (
                    INSERT INTO account_session (id, account_id)
                    VALUES ($1, $2)
                    ON CONFLICT (account_id) DO UPDATE
                    SET id = EXCLUDED.id
                    RETURNING account_id
                )
                SELECT
                    a.account_id,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    a.bio
                FROM account a
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                CROSS JOIN session_upsert
                WHERE a.account_id = $2
                "#,
            )
            .bind(session_id)
            .bind(address)
            .bind(&account.nickname)
            .bind(&account.image_uri)
            .bind(&account.bio)
       
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
