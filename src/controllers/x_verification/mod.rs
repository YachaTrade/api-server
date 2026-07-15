//! X (Twitter) hidden-creator verification.
//!
//! Postgres persistence layer: an idempotent transactional upsert of the
//! `token_x_verification` row plus its `token_x_followed_by` rows. See
//! `migrations/0036_token_x_verification.sql` (+ v2 upgrade track) for the
//! schema this operates on.

use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::types::x_verification::floor_followers;
use crate::{db::postgres::PostgresDatabase, types::token::x_verification::XFollowedByEntry};

pub struct XVerificationController {
    db: Arc<PostgresDatabase>,
}

/// Outcome of a `finalize` persist attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum FinalizeOutcome {
    /// Row inserted (first finalize) or refreshed (same X account).
    Persisted,
    /// A verification already exists for this token bound to a DIFFERENT
    /// x_user_id; nothing was changed.
    XAccountMismatch,
}

impl XVerificationController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Idempotent upsert of a verification + its followed_by rows (transaction).
    /// The X account (`x_user_id`) is locked on first finalize: a re-finalize
    /// bound to a different X account is rejected (`XAccountMismatch`) and
    /// leaves the existing row untouched; the same X account refreshes normally.
    pub async fn finalize(
        &self,
        token_id: &str,
        account_id: &str,
        x_user_id: &str,
        x_handle: &str,
        followers_count: i64,
        followed_by: &[XFollowedByEntry],
    ) -> Result<FinalizeOutcome> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|e| anyhow!("begin tx: {e}"))?;

        // Guarded upsert: x_user_id is deliberately NOT in the SET list — the WHERE
        // guarantees it already equals the incoming value, so it can never be
        // mutated. No existing row → INSERT → RETURNING Some. Existing row, same
        // x_user_id → DO UPDATE (WHERE true) → RETURNING Some. Existing row,
        // different x_user_id → WHERE false → 0 rows → RETURNING None.
        let persisted: Option<(String,)> = sqlx::query_as(
            r#"
            INSERT INTO token_x_verification (token_id, account_id, x_user_id, x_handle, followers_count)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (token_id) DO UPDATE
              SET account_id = EXCLUDED.account_id,
                  x_handle = EXCLUDED.x_handle,
                  followers_count = EXCLUDED.followers_count,
                  verified_at = NOW()
              WHERE token_x_verification.x_user_id = EXCLUDED.x_user_id
            RETURNING token_id
            "#,
        )
        .bind(token_id)
        .bind(account_id)
        .bind(x_user_id)
        .bind(x_handle)
        .bind(followers_count)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| anyhow!("upsert verification: {e}"))?;

        if persisted.is_none() {
            // Existing row bound to a different x_user_id → reject WITHOUT touching
            // followed_by. Dropping `tx` rolls back (the guarded upsert changed nothing).
            return Ok(FinalizeOutcome::XAccountMismatch);
        }

        sqlx::query("DELETE FROM token_x_followed_by WHERE token_id = $1")
            .bind(token_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow!("clear followed_by: {e}"))?;

        for fb in followed_by {
            sqlx::query(
                r#"
                INSERT INTO token_x_followed_by
                    (token_id, x_handle, x_image_uri, x_followers_count, is_x_verified)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (token_id, x_handle) DO NOTHING
                "#,
            )
            .bind(token_id)
            .bind(&fb.x_handle)
            .bind(&fb.x_image_uri)
            .bind(fb.x_followers_count)
            .bind(fb.is_x_verified)
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow!("insert followed_by: {e}"))?;
        }

        tx.commit().await.map_err(|e| anyhow!("commit: {e}"))?;
        Ok(FinalizeOutcome::Persisted)
    }

    /// Atomic first-writer-wins reservation. Returns the SURVIVING owner:
    /// the freshly-inserted account on success, or the pre-existing owner on
    /// conflict. The `DO UPDATE SET account_id = <its current value>` no-op
    /// forces `RETURNING` to yield the surviving row in one statement (no
    /// read-then-write race). The row is otherwise immutable.
    pub async fn reserve_first_writer(&self, token_id: &str, account_id: &str) -> Result<String> {
        let (owner,): (String,) = sqlx::query_as(
            r#"
            INSERT INTO token_x_reservation (token_id, account_id)
            VALUES ($1, $2)
            ON CONFLICT (token_id) DO UPDATE
                SET account_id = token_x_reservation.account_id
            RETURNING account_id
            "#,
        )
        .bind(token_id)
        .bind(account_id)
        .fetch_one(self.db.get_write_pool())
        .await
        .map_err(|e| anyhow!("reserve first-writer: {e}"))?;
        Ok(owner)
    }

    /// Read the reservation owner. Uses the WRITE pool so the finalize gate is
    /// read-after-write consistent (reserve → finalize can be near-simultaneous;
    /// a read replica could lag and wrongly report "not reserved").
    pub async fn reservation_owner(&self, token_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT account_id FROM token_x_reservation WHERE token_id = $1")
                .bind(token_id)
                .fetch_optional(self.db.get_write_pool())
                .await
                .map_err(|e| anyhow!("reservation owner: {e}"))?;
        Ok(row.map(|(a,)| a))
    }

    /// Read-only lookup for the public `/trade/xinfo/:token_id` endpoint.
    /// Returns `None` if this token has no finalized verification — this is
    /// NOT an error case (most tokens are unverified; verification is
    /// optional). Independent of whether `token_id` exists in the `token`
    /// table at all.
    pub async fn get_verification(
        &self,
        token_id: &str,
    ) -> Result<Option<crate::types::token::x_verification::TokenXVerification>> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool())
                .await
                .map_err(|e| anyhow!("get verification: {e}"))?;

        let Some((followers_count,)) = row else {
            return Ok(None);
        };

        #[derive(sqlx::FromRow)]
        struct FollowedByRow {
            x_handle: String,
            x_image_uri: String,
            x_followers_count: i64,
            is_x_verified: bool,
        }
        let followed_by_rows: Vec<FollowedByRow> = sqlx::query_as(
            r#"
            SELECT x_handle, x_image_uri, x_followers_count, is_x_verified
            FROM token_x_followed_by
            WHERE token_id = $1
            ORDER BY checked_at ASC
            "#,
        )
        .bind(token_id)
        .fetch_all(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow!("get followed_by: {e}"))?;

        Ok(Some(
            crate::types::token::x_verification::TokenXVerification {
                followers_count: floor_followers(followers_count),
                followed_by: followed_by_rows
                    .into_iter()
                    .map(|r| crate::types::token::x_verification::XFollowedByEntry {
                        x_handle: r.x_handle,
                        x_image_uri: r.x_image_uri,
                        x_followers_count: r.x_followers_count,
                        is_x_verified: r.is_x_verified,
                    })
                    .collect(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::token::x_verification::XFollowedByEntry;
    use sqlx::PgPool;

    const TOKEN: &str = "0x000000000000000000000000000000000000B143";
    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const ACCOUNT2: &str = "0x000000000000000000000000000000000000Aa02";

    fn ctrl(pool: PgPool) -> XVerificationController {
        XVerificationController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_persists_verification_and_followers(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "elonmusk".into(),
            x_image_uri: "https://img".into(),
            x_followers_count: 200_000_000,
            is_x_verified: true,
        }];
        ctrl(pool.clone())
            .finalize(TOKEN, ACCOUNT, "999", "creatorhandle", 128_500, &fb)
            .await
            .unwrap();

        let (fc,): (i64,) =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        // The DB keeps the EXACT (raw, non-round) value — flooring happens
        // only at the API response boundary (get_verification), not here.
        assert_eq!(fc, 128_500);

        let (creator_handle,): (Option<String>,) =
            sqlx::query_as("SELECT x_handle FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(creator_handle.as_deref(), Some("creatorhandle"));

        let (h,): (String,) =
            sqlx::query_as("SELECT x_handle FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(h, "elonmusk");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_is_idempotent(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "a".into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }];
        let c = ctrl(pool.clone());
        c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 10, &fb)
            .await
            .unwrap();
        c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 20, &fb)
            .await
            .unwrap(); // re-run
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1, "re-finalize must not duplicate followed_by rows");
        let (fc,): (i64,) =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fc, 20, "followers_count refreshed on re-finalize");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_removes_stale_followed_by_on_change(pool: PgPool) {
        let fb_ab = vec![
            XFollowedByEntry {
                x_handle: "a".into(),
                x_image_uri: "u".into(),
                x_followers_count: 1,
                is_x_verified: false,
            },
            XFollowedByEntry {
                x_handle: "b".into(),
                x_image_uri: "u".into(),
                x_followers_count: 2,
                is_x_verified: false,
            },
        ];
        let fb_a = vec![XFollowedByEntry {
            x_handle: "a".into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }];
        let c = ctrl(pool.clone());
        c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 10, &fb_ab)
            .await
            .unwrap();
        c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 10, &fb_a)
            .await
            .unwrap(); // "b" dropped from the list

        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1, "stale handle removed from followed_by on re-finalize");

        let b_gone: Option<(String,)> = sqlx::query_as(
            "SELECT x_handle FROM token_x_followed_by WHERE token_id = $1 AND x_handle = 'b'",
        )
        .bind(TOKEN)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(b_gone.is_none(), "handle 'b' should no longer be present");

        let (h,): (String,) =
            sqlx::query_as("SELECT x_handle FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(h, "a", "handle 'a' should remain");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_rejects_different_x_account(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "a".into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }];
        let c = ctrl(pool.clone());
        assert_eq!(
            c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 10, &fb)
                .await
                .unwrap(),
            FinalizeOutcome::Persisted
        );
        // re-finalize with a DIFFERENT x_user_id → rejected, nothing changes
        assert_eq!(
            c.finalize(TOKEN, ACCOUNT2, "2", "creatorhandle2", 20, &fb)
                .await
                .unwrap(),
            FinalizeOutcome::XAccountMismatch
        );
        let (account_id, x_user_id, x_handle, fc): (String, String, String, i64) = sqlx::query_as(
            "SELECT account_id, x_user_id, x_handle, followers_count FROM token_x_verification WHERE token_id = $1",
        )
        .bind(TOKEN)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            x_user_id, "1",
            "x_user_id stays locked to the first X account"
        );
        assert_eq!(
            x_handle, "creatorhandle",
            "x_handle unchanged on rejected re-finalize"
        );
        assert_eq!(
            account_id, ACCOUNT,
            "account_id unchanged on rejected re-finalize"
        );
        assert_eq!(fc, 10, "followers_count unchanged on rejected re-finalize");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_rejects_mismatch_without_wiping_followed_by(pool: PgPool) {
        let fb_a = vec![XFollowedByEntry {
            x_handle: "a".into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }];
        let c = ctrl(pool.clone());
        c.finalize(TOKEN, ACCOUNT, "1", "creatorhandle", 10, &fb_a)
            .await
            .unwrap();
        // different X account, even with an empty followed_by list → must NOT delete "a"
        assert_eq!(
            c.finalize(TOKEN, ACCOUNT2, "2", "creatorhandle2", 10, &[])
                .await
                .unwrap(),
            FinalizeOutcome::XAccountMismatch
        );
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1, "followed_by preserved when re-finalize is rejected");
        let (h,): (String,) =
            sqlx::query_as("SELECT x_handle FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(h, "a");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn reserve_is_first_writer_wins_and_idempotent(pool: PgPool) {
        let c = ctrl(pool.clone());
        // first writer wins
        let owner = c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(owner, ACCOUNT, "first reservation records the caller");
        // same account retry is idempotent (still owner)
        let owner2 = c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(owner2, ACCOUNT);
        // a different account cannot take it — returns the ORIGINAL owner
        let owner3 = c.reserve_first_writer(TOKEN, ACCOUNT2).await.unwrap();
        assert_eq!(
            owner3, ACCOUNT,
            "conflict returns the first writer, not the challenger"
        );
        // exactly one row, still owned by the first writer
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_reservation WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn reservation_owner_reads_back(pool: PgPool) {
        let c = ctrl(pool.clone());
        assert!(c.reservation_owner(TOKEN).await.unwrap().is_none());
        c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(
            c.reservation_owner(TOKEN).await.unwrap().as_deref(),
            Some(ACCOUNT)
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_verification_returns_none_when_unverified(pool: PgPool) {
        let c = ctrl(pool.clone());
        assert!(c.get_verification(TOKEN).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_verification_returns_nested_signals_when_verified(pool: PgPool) {
        let c = ctrl(pool.clone());
        let fb = vec![XFollowedByEntry {
            x_handle: "elonmusk".into(),
            x_image_uri: "https://img".into(),
            x_followers_count: 200_000_000,
            is_x_verified: true,
        }];
        c.finalize(TOKEN, ACCOUNT, "9", "creatorhandle", 128_500, &fb)
            .await
            .unwrap();

        // DB stores the raw 128_500, but get_verification floors it to
        // 128_000 at the output boundary — this asserts that flooring.
        let result = c.get_verification(TOKEN).await.unwrap();
        assert_eq!(
            result,
            Some(crate::types::token::x_verification::TokenXVerification {
                followers_count: 128_000,
                followed_by: vec![XFollowedByEntry {
                    x_handle: "elonmusk".into(),
                    x_image_uri: "https://img".into(),
                    x_followers_count: 200_000_000,
                    is_x_verified: true,
                }],
            })
        );
    }
}
