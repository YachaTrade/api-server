use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{db::postgres::PostgresDatabase, measure_postgres};

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

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<()> {
        measure_postgres!(
            "auth.set_session",
            sqlx::query(
                r#"
                INSERT INTO account_session (id, account_id)
                VALUES ($1, $2)
                ON CONFLICT (account_id) DO UPDATE
                SET id = EXCLUDED.id
                "#,
            )
            .bind(session_id)
            .bind(address)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set session: {}", err))?;

        Ok(())
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
