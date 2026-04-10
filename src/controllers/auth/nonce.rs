use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{db::postgres::PostgresDatabase, measure_postgres};

pub struct NonceController {
    pub db: Arc<PostgresDatabase>,
}

impl NonceController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        NonceController { db }
    }

    pub async fn set_nonce(&self, address: &str, message: &str, expiration_ms: u64) -> Result<()> {
        measure_postgres!(
            "auth.set_nonce",
            sqlx::query(
                r#"
                INSERT INTO auth_nonce (address, message, expires_at)
                VALUES ($1, $2, NOW() + make_interval(secs => $3))
                ON CONFLICT (address) DO UPDATE
                SET message = EXCLUDED.message,
                    expires_at = EXCLUDED.expires_at
                "#,
            )
            .bind(address)
            .bind(message)
            .bind(expiration_ms as f64 / 1000.0)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set nonce: {}", err))?;

        Ok(())
    }

    /// Atomically get and delete nonce to prevent reuse attacks
    pub async fn get_and_delete_nonce(&self, address: &str) -> Result<String> {
        let row: (String,) = measure_postgres!(
            "auth.get_and_delete_nonce",
            sqlx::query_as(
                r#"
                DELETE FROM auth_nonce
                WHERE address = $1 AND expires_at > NOW()
                RETURNING message
                "#,
            )
            .bind(address)
            .fetch_one(self.db.get_write_pool())
        )
        .map_err(|_| anyhow!("Message not found or already used"))?;

        Ok(row.0)
    }
}
