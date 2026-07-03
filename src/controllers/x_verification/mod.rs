//! X (Twitter) hidden-creator verification.
//!
//! Postgres persistence layer: an idempotent transactional upsert of the
//! `token_x_verification` row plus its `token_x_followed_by` rows. See
//! `migrations/0036_token_x_verification.sql` (+ v2 upgrade track) for the
//! schema this operates on.

use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{db::postgres::PostgresDatabase, types::token::x_verification::XFollowedByEntry};

pub struct XVerificationController {
    db: Arc<PostgresDatabase>,
}

impl XVerificationController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Idempotent upsert of a verification + its followed_by rows (transaction).
    pub async fn finalize(
        &self,
        token_id: &str,
        account_id: &str,
        x_user_id: &str,
        followers_count: i64,
        followed_by: &[XFollowedByEntry],
    ) -> Result<()> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|e| anyhow!("begin tx: {e}"))?;

        sqlx::query(
            r#"
            INSERT INTO token_x_verification (token_id, account_id, x_user_id, followers_count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (token_id) DO UPDATE
              SET account_id = EXCLUDED.account_id,
                  x_user_id = EXCLUDED.x_user_id,
                  followers_count = EXCLUDED.followers_count,
                  verified_at = NOW()
            "#,
        )
        .bind(token_id)
        .bind(account_id)
        .bind(x_user_id)
        .bind(followers_count)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("upsert verification: {e}"))?;

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
        Ok(())
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
                followers_count,
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
            .finalize(TOKEN, ACCOUNT, "999", 128_000, &fb)
            .await
            .unwrap();

        let (fc,): (i64,) =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fc, 128_000);

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
        c.finalize(TOKEN, ACCOUNT, "1", 10, &fb).await.unwrap();
        c.finalize(TOKEN, ACCOUNT, "1", 20, &fb).await.unwrap(); // re-run
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
        c.finalize(TOKEN, ACCOUNT, "1", 10, &fb_ab).await.unwrap();
        c.finalize(TOKEN, ACCOUNT, "1", 10, &fb_a).await.unwrap(); // "b" dropped from the list

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
    async fn finalize_refreshes_account_and_user_id_on_conflict(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "a".into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }];
        let c = ctrl(pool.clone());
        c.finalize(TOKEN, ACCOUNT, "1", 10, &fb).await.unwrap();
        let (verified_at_1,): (chrono::DateTime<chrono::Utc>,) =
            sqlx::query_as("SELECT verified_at FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();

        c.finalize(TOKEN, ACCOUNT2, "2", 10, &fb).await.unwrap(); // re-run with a different account/user

        let (account_id, x_user_id, verified_at_2): (
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ) = sqlx::query_as(
            "SELECT account_id, x_user_id, verified_at FROM token_x_verification WHERE token_id = $1",
        )
        .bind(TOKEN)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(account_id, ACCOUNT2, "account_id refreshed on re-finalize");
        assert_eq!(x_user_id, "2", "x_user_id refreshed on re-finalize");
        assert!(
            verified_at_2 > verified_at_1,
            "verified_at should advance on re-finalize"
        );
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
        c.finalize(TOKEN, ACCOUNT, "9", 128_000, &fb).await.unwrap();

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
