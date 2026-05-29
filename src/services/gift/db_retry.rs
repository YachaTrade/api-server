//! Transient-error retry helper for DB queries.
//!
//! Distinguishes *transport faults we should try again* (pool
//! saturation, connection drops, Postgres class `08xxx` connection
//! exceptions) from *logic faults that should bubble immediately*
//! (syntax errors, constraint violations, permission denials). The
//! latter never get better on retry — retrying hides bugs.
//!
//! Pairs with `sqlx::PgPool`'s built-in reconnect behaviour: when a
//! connection is dropped, the pool reconnects on next `acquire()`,
//! which our retry loop exercises.

use std::time::Duration;

use tracing::warn;

/// Maximum number of attempts for [`with_retry`] (including the first
/// call). After this the last error bubbles to the caller.
pub const MAX_ATTEMPTS: u32 = 3;

/// Linear backoff base: attempt N sleeps for `BACKOFF_BASE_MS * N` ms
/// before retrying. Short by design — DB transients usually clear in
/// milliseconds.
const BACKOFF_BASE_MS: u64 = 100;

/// Run a fallible DB op, retrying up to [`MAX_ATTEMPTS`] times on
/// transient failures (see [`is_transient`]).
///
/// Parameters:
/// - `op_name`: short identifier used in the `warn!` log on retry.
///   Keep it to snake_case so dashboards can group.
/// - `f`: async closure producing a fresh `sqlx::Result<T>` each call.
///   Must be idempotent — we may call it more than once.
///
/// Logs a `warn!` per retry (including attempt number, delay, and
/// error) so operators see transient blips without turning `info`
/// on. Final errors bubble — we don't re-wrap them.
pub async fn with_retry<T, F, Fut>(op_name: &'static str, mut f: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = sqlx::Result<T>>,
{
    let mut attempt: u32 = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if is_transient(&e) && attempt + 1 < MAX_ATTEMPTS => {
                attempt += 1;
                let delay = Duration::from_millis(BACKOFF_BASE_MS * attempt as u64);
                warn!(
                    op = op_name,
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "transient DB error; retrying"
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// True if `err` represents a transport-level failure that may succeed
/// on retry. The current rules, in priority order:
///
/// | Error | Transient? | Rationale |
/// |-------|------------|-----------|
/// | `Io(_)`                   | yes | network glitch / broken pipe |
/// | `PoolTimedOut`            | yes | saturation; next attempt likely finds a free conn |
/// | `PoolClosed`              | no  | pool is being shut down — no point retrying |
/// | `Database` with SQLSTATE class `08` | yes | "Connection Exception" per SQL standard |
/// | any other `Database`      | no  | syntax, constraint, permission, out-of-memory — retries don't help |
/// | everything else           | no  | conservative default |
pub fn is_transient(err: &sqlx::Error) -> bool {
    use sqlx::Error;
    match err {
        Error::Io(_) => true,
        Error::PoolTimedOut => true,
        Error::PoolClosed => false,
        Error::Database(db_err) => db_err.code().map(|c| c.starts_with("08")).unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn io_errors_are_transient() {
        let err = sqlx::Error::Io(IoError::new(ErrorKind::ConnectionReset, "reset"));
        assert!(is_transient(&err));
    }

    #[test]
    fn pool_timeout_is_transient() {
        let err = sqlx::Error::PoolTimedOut;
        assert!(is_transient(&err));
    }

    #[test]
    fn pool_closed_is_not_transient() {
        let err = sqlx::Error::PoolClosed;
        assert!(!is_transient(&err));
    }

    #[test]
    fn row_not_found_is_not_transient() {
        // Shape of a logical "nothing to return" — not a transport
        // issue. Callers that tolerate this should match explicitly,
        // not retry blindly.
        let err = sqlx::Error::RowNotFound;
        assert!(!is_transient(&err));
    }

    #[tokio::test]
    async fn with_retry_returns_first_ok_without_sleeping() {
        let start = std::time::Instant::now();
        let result: Result<i32, sqlx::Error> = with_retry("test_ok", || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn with_retry_gives_up_on_terminal_error() {
        // `RowNotFound` is non-transient — must bubble immediately
        // without exhausting the 3-attempt budget.
        let mut calls = 0;
        let result: Result<i32, sqlx::Error> = with_retry("test_terminal", || {
            calls += 1;
            async { Err(sqlx::Error::RowNotFound) }
        })
        .await;
        assert!(matches!(result, Err(sqlx::Error::RowNotFound)));
        assert_eq!(calls, 1, "non-transient error must not retry");
    }

    #[tokio::test]
    async fn with_retry_exhausts_on_persistent_transient() {
        // Always-transient error — retries until MAX_ATTEMPTS then
        // bubbles the final error. Verifies we don't loop forever.
        let mut calls = 0;
        let result: Result<i32, sqlx::Error> = with_retry("test_exhaust", || {
            calls += 1;
            async { Err(sqlx::Error::PoolTimedOut) }
        })
        .await;
        assert!(matches!(result, Err(sqlx::Error::PoolTimedOut)));
        assert_eq!(
            calls, MAX_ATTEMPTS as usize,
            "should attempt exactly MAX_ATTEMPTS times"
        );
    }

    #[tokio::test]
    async fn with_retry_recovers_after_transient_blip() {
        // First call transient, second succeeds — simulates a network
        // flap that clears on reconnect.
        let mut calls = 0;
        let result: Result<i32, sqlx::Error> = with_retry("test_recover", || {
            calls += 1;
            let current = calls;
            async move {
                if current == 1 {
                    Err(sqlx::Error::PoolTimedOut)
                } else {
                    Ok(99)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 99);
        assert_eq!(calls, 2);
    }
}
