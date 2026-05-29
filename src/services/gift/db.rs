//! gift_tweet queries. Ported from gift-bot producer/consumer; uses api-server's
//! shared Postgres pool. Address columns are VARCHAR → lowercase 0x… text on write.
//!
//! All queries use the shared pool from api-server's `AppState`. Transient errors
//! (pool saturation, dropped connections) are retried at the call-site
//! via `crate::services::gift::db_retry::with_retry`; non-transient errors bubble so the
//! caller can log + skip or exit.
//!
//! The `handle`, `token_id`, and `receiver_id` columns are `VARCHAR`
//! because the shared `migrations` submodule uses text addresses. This
//! module converts to alloy `Address` on read and back to `0x…` on
//! write so callers see typed values.

use std::str::FromStr;

use alloy::primitives::Address;
use sqlx::PgPool;
use thiserror::Error;

use crate::services::gift::domain::ParsedGift;

/// Idempotent insert from the webhook ingest path. Mirrors gift-bot
/// producer/ingest.rs: ON CONFLICT (tweet_id) DO NOTHING. The
/// gift_tweet_notify trigger fires pg_notify('gift_tweet_new') on insert.
/// Returns true if a row was inserted (false = duplicate).
pub async fn insert_gift_tweet(pool: &PgPool, gift: &ParsedGift) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "INSERT INTO gift_tweet (tweet_id, token_id, receiver_id, handle) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (tweet_id) DO NOTHING",
    )
    .bind(&gift.tweet_id)
    .bind(format!("{:#x}", gift.token))
    .bind(format!("{:#x}", gift.receiver))
    .bind(&gift.author_username)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Ingest-side allowlist: is `token` present in the indexer's `token` table as
/// a V2 token? Mirrors gift-bot producer's `token_indexed` pre-filter — an
/// optimization (NOT a security boundary; the consumer's on-chain `getGiftInfo`
/// is authoritative) that keeps `gift_tweet` from accumulating reject rows for
/// tweets naming tokens that were never created on V2.
///
/// Adapted to api-server's address convention: the `token.token_id` column
/// stores EIP-55 checksummed addresses and is compared exactly (LOWER() is
/// forbidden project-wide), so we checksum the parsed address before binding.
/// Wrapped in `with_retry` so a transient blip is retried rather than dropping
/// a legitimate tweet; the caller fails open on a terminal error.
pub async fn token_indexed(pool: &PgPool, token: Address) -> Result<bool, sqlx::Error> {
    let token_id = token.to_checksum(None);
    crate::services::gift::db_retry::with_retry("token_indexed", || {
        let pool = pool.clone();
        let token_id = token_id.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM token WHERE token_id = $1 AND version = 'V2')",
            )
            .bind(&token_id)
            .fetch_one(&pool)
            .await
        }
    })
    .await
}

/// A row in `gift_tweet` projected into typed columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftRow {
    pub tweet_id: String,
    pub token_id: Address,
    pub receiver_id: Address,
    pub handle: String,
    pub status: String,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("invalid address in DB column `{column}`: {raw}")]
    InvalidAddress { column: &'static str, raw: String },
}

/// Claim one row in `pending` status for processing. Runs inside a
/// caller-owned transaction so the row-lock `FOR UPDATE SKIP LOCKED`
/// holds for the duration of preflight + submit.
///
/// Returns `Ok(None)` if nothing is pending; the caller should commit
/// and wait for the next signal.
pub async fn claim_pending<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
) -> Result<Option<GiftRow>, DbError> {
    let raw = sqlx::query_as::<_, RawGiftRow>(
        r#"
        SELECT tweet_id, token_id, receiver_id, handle, status, tx_hash
          FROM gift_tweet
         WHERE status = 'pending'
         ORDER BY received_at ASC
         LIMIT 1
         FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(&mut **tx)
    .await?;

    raw.map(GiftRow::try_from).transpose()
}

/// List every row currently in `status='submitted'`. Ordered by age so
/// the oldest goes first — minimizes the window where a stuck row can
/// re-enter the queue.
pub async fn list_submitted(pool: &PgPool) -> Result<Vec<GiftRow>, DbError> {
    let raw = sqlx::query_as::<_, RawGiftRow>(
        r#"
        SELECT tweet_id, token_id, receiver_id, handle, status, tx_hash
          FROM gift_tweet
         WHERE status = 'submitted'
         ORDER BY received_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    raw.into_iter().map(GiftRow::try_from).collect()
}

/// Oldest `submitted` row that has a `tx_hash` recorded. Runtime
/// reconciliation uses this: rows with `tx_hash IS NULL` are the
/// "claimed, pre-send" window and are left to the owning consumer to
/// finish (or to boot-sweep on a crash). Ordering by `updated_at` means
/// we keep re-reconciling the least-recently touched row first.
pub async fn oldest_submitted_with_hash(pool: &PgPool) -> Result<Option<GiftRow>, DbError> {
    let raw = sqlx::query_as::<_, RawGiftRow>(
        r#"
        SELECT tweet_id, token_id, receiver_id, handle, status, tx_hash
          FROM gift_tweet
         WHERE status = 'submitted'
           AND tx_hash IS NOT NULL
         ORDER BY updated_at ASC
         LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    raw.map(GiftRow::try_from).transpose()
}

/// Mark a row `rejected` with a reason. `reason` is the varchar(32)
/// label — pass `RejectReason::as_str()` or the `"Reverted"` constant.
pub async fn mark_rejected(pool: &PgPool, tweet_id: &str, reason: &str) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET status = 'rejected',
               reject_reason = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition a row from `pending` → `submitted` with its tx hash.
pub async fn mark_submitted(pool: &PgPool, tweet_id: &str, tx_hash: &str) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET status = 'submitted',
               tx_hash = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(tx_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition a row to `completed`. Keeps `tx_hash` intact (it was set
/// on the prior `submitted` transition).
///
/// If `queue_reply` is true, also flips `reply_status` from its default
/// `'none'` to `'pending'`, putting the row on the reply-worker's queue.
/// Passing `false` (caller's `ReplyConfig` absent) leaves the column at
/// `'none'`, so the reply worker — if later enabled — won't surprise the
/// operator with a backlog of stale replies on rows finalized before the
/// feature was turned on.
pub async fn mark_completed(
    pool: &PgPool,
    tweet_id: &str,
    queue_reply: bool,
) -> Result<(), DbError> {
    if queue_reply {
        sqlx::query(
            r#"
            UPDATE gift_tweet
               SET status = 'completed',
                   reply_status = 'pending',
                   updated_at = NOW()
             WHERE tweet_id = $1
            "#,
        )
        .bind(tweet_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE gift_tweet
               SET status = 'completed',
                   updated_at = NOW()
             WHERE tweet_id = $1
            "#,
        )
        .bind(tweet_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Reset a `submitted` row back to `pending` (e.g. tx was dropped from
/// mempool). Clears `tx_hash` so the next cycle signs a fresh one, and
/// writes `last_error` so operators can see the reason.
pub async fn reset_to_pending(
    pool: &PgPool,
    tweet_id: &str,
    last_error: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET status = 'pending',
               tx_hash = NULL,
               last_error = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a transient error string for a row without transitioning
/// status. Used when preflight / send / receipt fetch fails in a way
/// that's retryable.
pub async fn set_last_error(
    pool: &PgPool,
    tweet_id: &str,
    last_error: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET last_error = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------- Reply queue (reply-on-success worker) ----------

/// One row's worth of reply-relevant state. The worker only needs the
/// tweet id (X reply target), receiver address (template substitution),
/// and attempt count (caller decides to bump-or-give-up). Keeping this a
/// distinct struct from `GiftRow` means the SELECT stays a covering
/// fetch over just the columns we need.
#[derive(Debug, Clone)]
pub struct ReplyRow {
    pub tweet_id: String,
    pub receiver_id: alloy::primitives::Address,
    pub attempts: i32,
}

/// Atomically claim one pending-reply row, prioritizing oldest first.
/// `FOR UPDATE SKIP LOCKED` lets us scale to N workers in the future
/// without doubling tweets up; today we run one but the lock semantics
/// cost nothing and avoid the trap.
pub async fn claim_one_reply_pending(pool: &PgPool) -> Result<Option<ReplyRow>, DbError> {
    let r: Option<(String, String, i32)> = sqlx::query_as(
        r#"
        SELECT tweet_id, receiver_id, reply_attempts
          FROM gift_tweet
         WHERE reply_status = 'pending'
         ORDER BY updated_at ASC
         LIMIT 1
         FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(match r {
        None => None,
        Some((tweet_id, receiver_id, attempts)) => {
            let receiver_id =
                Address::from_str(&receiver_id).map_err(|_| DbError::InvalidAddress {
                    column: "receiver_id",
                    raw: receiver_id.clone(),
                })?;
            Some(ReplyRow {
                tweet_id,
                receiver_id,
                attempts,
            })
        }
    })
}

/// Finalize a row as replied-to. `reply_tweet_id` is the new tweet X
/// returned from `POST /2/tweets` — kept for audit and reply-link UI.
pub async fn mark_reply_sent(
    pool: &PgPool,
    tweet_id: &str,
    reply_tweet_id: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET reply_status = 'sent',
               reply_tweet_id = $2,
               reply_sent_at = NOW(),
               reply_last_error = NULL,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(reply_tweet_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment `reply_attempts` and record the latest error. Status stays
/// `'pending'` so the worker picks the row up again after the backoff.
pub async fn bump_reply_attempt(
    pool: &PgPool,
    tweet_id: &str,
    last_error: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET reply_attempts = reply_attempts + 1,
               reply_last_error = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Terminal failure — caller stopped retrying. Reasons:
/// `ReplyError::is_retryable() == false`, or `attempts >= max_attempts`.
/// Operator can grep `reply_status='failed'` and `reply_last_error` to
/// debug; nothing else looks at these rows.
pub async fn mark_reply_failed(
    pool: &PgPool,
    tweet_id: &str,
    last_error: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE gift_tweet
           SET reply_status = 'failed',
               reply_last_error = $2,
               updated_at = NOW()
         WHERE tweet_id = $1
        "#,
    )
    .bind(tweet_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------- /Reply queue ----------

/// sqlx row type before Address parsing.
#[derive(sqlx::FromRow)]
struct RawGiftRow {
    tweet_id: String,
    token_id: String,
    receiver_id: String,
    handle: String,
    status: String,
    tx_hash: Option<String>,
}

impl TryFrom<RawGiftRow> for GiftRow {
    type Error = DbError;
    fn try_from(r: RawGiftRow) -> Result<Self, Self::Error> {
        let token_id = Address::from_str(&r.token_id).map_err(|_| DbError::InvalidAddress {
            column: "token_id",
            raw: r.token_id.clone(),
        })?;
        let receiver_id =
            Address::from_str(&r.receiver_id).map_err(|_| DbError::InvalidAddress {
                column: "receiver_id",
                raw: r.receiver_id.clone(),
            })?;
        Ok(Self {
            tweet_id: r.tweet_id,
            token_id,
            receiver_id,
            handle: r.handle,
            status: r.status,
            tx_hash: r.tx_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_row_parses_into_typed_addresses() {
        let raw = RawGiftRow {
            tweet_id: "123".into(),
            token_id: "0x0000000000000000000000000000000000000001".into(),
            receiver_id: "0x0000000000000000000000000000000000000002".into(),
            handle: "alice".into(),
            status: "pending".into(),
            tx_hash: None,
        };
        let row = GiftRow::try_from(raw).unwrap();
        assert_eq!(row.tweet_id, "123");
        assert_eq!(
            row.token_id,
            Address::from_str("0x0000000000000000000000000000000000000001").unwrap()
        );
        assert_eq!(row.handle, "alice");
    }

    #[test]
    fn invalid_address_surfaces_clear_error() {
        let raw = RawGiftRow {
            tweet_id: "123".into(),
            token_id: "not-an-address".into(),
            receiver_id: "0x0000000000000000000000000000000000000002".into(),
            handle: "alice".into(),
            status: "pending".into(),
            tx_hash: None,
        };
        let err = GiftRow::try_from(raw).unwrap_err();
        match err {
            DbError::InvalidAddress { column, raw } => {
                assert_eq!(column, "token_id");
                assert_eq!(raw, "not-an-address");
            }
            other => panic!("expected InvalidAddress, got {other:?}"),
        }
    }
}
