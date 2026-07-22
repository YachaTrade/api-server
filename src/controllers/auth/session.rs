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
                    a.nickname as nickname,
                    a.image_uri as image_uri,
                    a.bio
                -- Read from the account_upsert CTE, not the base `account` table:
                -- data-modifying CTEs run under the statement's start snapshot, so a
                -- just-inserted brand-new account is invisible to `FROM account` and
                -- the query would return 0 rows (RowNotFound) on first login.
                FROM account_upsert a
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    // EIP-55 checksummed addresses (VARCHAR(42))
    const NEW_ACCOUNT: &str = "0x000000000000000000000000000000000000Ab01";
    const EXISTING_ACCOUNT: &str = "0x000000000000000000000000000000000000Ab02";
    const SESSION_ID: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const SESSION_ID2: &str = "0000000000000000000000000000000000000000000000000000000000000002";

    fn make_controller(pool: PgPool) -> SessionController {
        SessionController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    /// `AccountInfo::new` reads `DEFAULT_IMAGE_{1..=5}` and panics if unset.
    fn set_default_image_env() {
        for i in 1..=5 {
            // SAFETY: tests set the same constant value; no concurrent reader depends
            // on it being unset.
            unsafe { std::env::set_var(format!("DEFAULT_IMAGE_{i}"), "default.png") };
        }
    }

    async fn session_account_id(pool: &PgPool, session_id: &str) -> Option<String> {
        sqlx::query_as::<_, SessionRow>("SELECT account_id FROM account_session WHERE id = $1")
            .bind(session_id)
            .fetch_optional(pool)
            .await
            .unwrap()
            .map(|r| r.account_id)
    }

    /// Root cause regression: a brand-new wallet (no pre-existing `account` row)
    /// logging in for the first time. The final `SELECT` must see the row the
    /// `account_upsert` CTE just inserted, otherwise `fetch_one` hits RowNotFound
    /// → InternalError("Database error: no rows returned ...").
    #[sqlx::test(migrations = "./migrations")]
    async fn set_session_succeeds_for_brand_new_account(pool: PgPool) {
        set_default_image_env();
        let ctrl = make_controller(pool.clone());

        let account_info = ctrl
            .set_session(SESSION_ID, NEW_ACCOUNT)
            .await
            .expect("set_session must succeed for a brand-new account");

        assert_eq!(account_info.account_id, NEW_ACCOUNT);
        assert_eq!(
            session_account_id(&pool, SESSION_ID).await.as_deref(),
            Some(NEW_ACCOUNT),
            "account_session row must be created for the new account"
        );
    }

    /// Returning user: the `account` row already exists. The upsert must return the
    /// stored profile (not the generated default) and rotate the session id via
    /// `ON CONFLICT (account_id) DO UPDATE`.
    #[sqlx::test(migrations = "./migrations")]
    async fn set_session_updates_session_for_existing_account(pool: PgPool) {
        set_default_image_env();
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri) VALUES ($1, 'stored_nick', 'stored bio', 'stored.png')",
        )
        .bind(EXISTING_ACCOUNT)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO account_session (id, account_id) VALUES ($1, $2)")
            .bind(SESSION_ID)
            .bind(EXISTING_ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();

        let ctrl = make_controller(pool.clone());
        let account_info = ctrl
            .set_session(SESSION_ID2, EXISTING_ACCOUNT)
            .await
            .expect("set_session must succeed for an existing account");

        // returns the stored profile, not AccountInfo::new defaults
        assert_eq!(account_info.account_id, EXISTING_ACCOUNT);
        assert_eq!(account_info.nickname, "stored_nick");
        assert_eq!(account_info.bio, "stored bio");
        // session id rotated to the new one
        assert_eq!(
            session_account_id(&pool, SESSION_ID2).await.as_deref(),
            Some(EXISTING_ACCOUNT),
            "new session id must point to the account"
        );
        assert_eq!(
            session_account_id(&pool, SESSION_ID).await,
            None,
            "old session id must be replaced (one session row per account)"
        );
    }
}
