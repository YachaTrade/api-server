use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::{db::postgres::PostgresDatabase, measure_postgres};

/// One retry after this delay covers cross-region replication lag: the GET /nonce write
/// may not have replicated to the node serving POST /session yet.
const NONCE_REPLICATION_RETRY_DELAY: Duration = Duration::from_secs(2);

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

    /// Atomically get and delete nonce to prevent reuse attacks.
    ///
    /// If the nonce is not present on the first read it may simply not have replicated
    /// to this region yet, so we wait once and retry before rejecting.
    pub async fn get_and_delete_nonce(
        &self,
        address: &str,
        expected_message: &str,
    ) -> Result<String> {
        self.take_nonce(address, expected_message, NONCE_REPLICATION_RETRY_DELAY)
            .await
    }

    async fn take_nonce(
        &self,
        address: &str,
        expected_message: &str,
        retry_delay: Duration,
    ) -> Result<String> {
        if let Some(message) = self.try_take_nonce(address, expected_message).await? {
            return Ok(message);
        }

        // Not visible yet — could be cross-region replication lag. Wait once, then retry.
        tokio::time::sleep(retry_delay).await;

        self.try_take_nonce(address, expected_message)
            .await?
            .ok_or_else(|| anyhow!("Message not found or already used"))
    }

    /// Atomic consume: deletes the (non-expired) nonce matching the signed message and
    /// returns it, or `None` if no matching row exists. Matching on the expected message
    /// keeps the retry from eating a *newer* nonce issued for the same address during the
    /// wait window. A real DB error still propagates as `Err`.
    async fn try_take_nonce(
        &self,
        address: &str,
        expected_message: &str,
    ) -> Result<Option<String>> {
        let row: Option<(String,)> = measure_postgres!(
            "auth.get_and_delete_nonce",
            sqlx::query_as(
                r#"
                DELETE FROM auth_nonce
                WHERE address = $1 AND message = $2 AND expires_at > NOW()
                RETURNING message
                "#,
            )
            .bind(address)
            .bind(expected_message)
            .fetch_optional(self.db.get_write_pool())
        )?;

        Ok(row.map(|r| r.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use std::time::Duration;

    const ADDRESS: &str = "0x000000000000000000000000000000000000Ac01";
    const MESSAGE: &str = "sign-this-nonce-message";
    const OTHER_MESSAGE: &str = "a-different-newer-nonce";

    fn make_controller(pool: PgPool) -> NonceController {
        NonceController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn insert_nonce(pool: &PgPool, ttl: &str) {
        insert_nonce_msg(pool, MESSAGE, ttl).await;
    }

    async fn insert_nonce_msg(pool: &PgPool, message: &str, ttl: &str) {
        sqlx::query(&format!(
            "INSERT INTO auth_nonce (address, message, expires_at) VALUES ($1, $2, NOW() + interval '{ttl}')"
        ))
        .bind(ADDRESS)
        .bind(message)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn stored_message(pool: &PgPool) -> Option<String> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT message FROM auth_nonce WHERE address = $1")
                .bind(ADDRESS)
                .fetch_optional(pool)
                .await
                .unwrap();
        row.map(|r| r.0)
    }

    /// Cross-region replication lag: GET /nonce wrote the row on another node, so it is
    /// not yet visible when POST /session consumes it here. get_and_delete_nonce must wait
    /// and retry instead of rejecting with a spurious "Invalid nonce".
    #[sqlx::test(migrations = "./migrations")]
    async fn nonce_replicated_late_is_recovered_by_retry(pool: PgPool) {
        let ctrl = make_controller(pool.clone());

        // The replicated row lands ~150ms after the consume begins (well inside the retry window).
        let writer = pool.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            insert_nonce(&writer, "5 minutes").await;
        });

        let got = ctrl
            .get_and_delete_nonce(ADDRESS, MESSAGE)
            .await
            .expect("retry should recover a late-replicated nonce");
        assert_eq!(got, MESSAGE);
    }

    /// Genuinely missing nonce: still rejected after the retry (replication is not magic).
    #[sqlx::test(migrations = "./migrations")]
    async fn take_nonce_still_fails_when_absent_after_retry(pool: PgPool) {
        let ctrl = make_controller(pool);
        let res = ctrl
            .take_nonce(ADDRESS, MESSAGE, Duration::from_millis(20))
            .await;
        assert!(res.is_err(), "absent nonce must fail even after retry");
    }

    /// Present on the first read: consumed immediately and deleted (reuse prevention).
    #[sqlx::test(migrations = "./migrations")]
    async fn take_nonce_consumes_existing_nonce(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        insert_nonce(&pool, "5 minutes").await;

        let got = ctrl
            .take_nonce(ADDRESS, MESSAGE, Duration::from_millis(20))
            .await
            .unwrap();
        assert_eq!(got, MESSAGE);
        assert!(
            stored_message(&pool).await.is_none(),
            "nonce must be deleted after consume"
        );
    }

    /// Expired nonce stays rejected — the retry must not resurrect it.
    #[sqlx::test(migrations = "./migrations")]
    async fn take_nonce_rejects_expired_nonce(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        insert_nonce(&pool, "-1 minute").await; // already expired

        let res = ctrl
            .take_nonce(ADDRESS, MESSAGE, Duration::from_millis(20))
            .await;
        assert!(res.is_err(), "expired nonce must be rejected");
    }

    /// Regression (codex P2): if a *newer* nonce is issued for the same address during the
    /// retry window, the consume must NOT eat it. We only consume the exact message the
    /// client signed; the unrelated newer nonce stays put for its own login attempt.
    #[sqlx::test(migrations = "./migrations")]
    async fn retry_does_not_consume_a_different_nonce(pool: PgPool) {
        let ctrl = make_controller(pool.clone());
        // Only a different (newer) nonce exists; our request signed MESSAGE.
        insert_nonce_msg(&pool, OTHER_MESSAGE, "5 minutes").await;

        let res = ctrl
            .take_nonce(ADDRESS, MESSAGE, Duration::from_millis(20))
            .await;

        assert!(res.is_err(), "must not consume a nonce we didn't sign");
        assert_eq!(
            stored_message(&pool).await.as_deref(),
            Some(OTHER_MESSAGE),
            "the unrelated newer nonce must remain for its own login"
        );
    }
}
