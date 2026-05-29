//! Consumer poller — LISTEN + interval fallback.
//!
//! Loop body:
//! 1. Wait for either a `gift_tweet_new` NOTIFY or the
//!    `CONSUMER_POLL_INTERVAL_MS` tick.
//! 2. Open a txn, `SELECT … FOR UPDATE SKIP LOCKED` one pending row.
//!    If the claim is empty, commit and loop.
//! 3. Preflight — on reject, update DB inside the txn and commit.
//!    On transient error, write `last_error` and commit (row stays
//!    pending).
//! 4. On `PreflightOutcome::Send`, commit the claim (releases the
//!    row-lock) and hand off to the executor. The executor writes the
//!    `submitted` transition inside its own pipeline so the tx_hash is
//!    durable even if the process dies mid-wait.
//! 5. Executor returns an outcome; poller maps it to the final DB write
//!    (completed / rejected / reset to pending / stay submitted).
//!
//! PgListener reconnect: any error from `recv()` is treated as a dead
//! connection — we sleep a short backoff, re-establish the listener,
//! and continue. Shutdown is checked every iteration so SIGTERM drains
//! cleanly.

use std::str::FromStr;
use std::time::Duration;

use crate::services::gift::db_retry::with_retry;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::services::gift::db;
use crate::services::gift::executor::{ExecutionOutcome, Executor};
use crate::services::gift::reconcile::REVERTED_REASON;
use crate::chain::rpc_chain::RpcChain;
use crate::services::gift::validator::{PreflightOutcome, preflight};

/// NOTIFY channel name — matches the `notify_gift_tweet_new` trigger in
/// `migrations/0020_gift_tweet.sql`.
const NOTIFY_CHANNEL: &str = "gift_tweet_new";

/// Shape of a single claim cycle's outcome. Returned by `try_claim_and_process`
/// so the outer loop can decide whether to immediately try another claim
/// or wait for the next signal.
enum ClaimResult {
    /// A row was processed — try another claim right away (the DB likely
    /// has more pending rows queued behind the one we just handled).
    Processed,
    /// No pending rows; wait for the next signal.
    Empty,
    /// A transient error (preflight RPC blip, pre-submit failure) left the
    /// row pending. Stop draining this wakeup and wait for the next poll
    /// tick — that interval acts as backoff and prevents a tight re-claim
    /// burst of the same oldest pending row during an RPC/DB outage.
    Backoff,
    /// Transient DB error while claiming — treat like Empty but log.
    DbError(String),
}

/// Long-lived poller state wired by `main.rs`.
pub struct Poller {
    pub pool: PgPool,
    pub chain: RpcChain,
    pub executor: Executor,
    pub gift_vault: alloy::primitives::Address,
    pub poll_interval: Duration,
    pub database_url: String,
    /// Mirrors `ConsumerConfig::reply.is_some()` — when true, runtime
    /// reconciliation's `mark_completed` also queues the row for the
    /// reply worker. False keeps the legacy behavior.
    pub reply_enabled: bool,
    /// Wait window applied after claiming a row and before preflight.
    /// Zero (default) is the primary consumer's behavior — process
    /// immediately. A fallback instance sets this to e.g. 30s so the
    /// primary has time to call `setReceiver` first; when the wait
    /// expires, preflight's `AlreadyBound` reject path naturally skips
    /// rows the primary already wrote on-chain during the sleep.
    pub consumer_wait_time: Duration,
}

impl Poller {
    /// Run the poll/listen loop until `shutdown` fires. Returns only on
    /// shutdown — transient errors are logged + retried inline.
    pub async fn run(self, shutdown: CancellationToken) {
        info!(
            poll_interval_ms = self.poll_interval.as_millis() as u64,
            "consumer poller starting"
        );

        let mut listener = match connect_listener(&self.database_url, &shutdown).await {
            Some(l) => Some(l),
            None => {
                warn!("initial PgListener connect failed; running interval-only until reconnect");
                None
            }
        };

        let mut tick = interval(self.poll_interval);
        // If the runtime stalls, don't catch up by firing a flurry of
        // ticks — drain once and move on.
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            if shutdown.is_cancelled() {
                info!("shutdown observed; poller exiting");
                return;
            }

            // Wait for either a NOTIFY, the fallback tick, or shutdown.
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("shutdown during poll wait");
                    return;
                }
                notify = async {
                    match &mut listener {
                        Some(l) => l.recv().await.map(Some),
                        // Pending future when listener is None — forces the
                        // other arms to be the only sources of wakeup.
                        None => std::future::pending::<Result<Option<sqlx::postgres::PgNotification>, sqlx::Error>>().await,
                    }
                } => match notify {
                    Ok(Some(n)) => {
                        debug!(
                            channel = n.channel(),
                            payload = n.payload(),
                            "NOTIFY received"
                        );
                    }
                    Ok(None) => {
                        // try_recv signals a reconnect in progress; fall
                        // through to the claim step.
                    }
                    Err(e) => {
                        warn!(error = %e, "PgListener recv failed; reconnecting");
                        listener = None;
                        // Back-off before reconnect so we don't hammer a
                        // dead Postgres.
                        if sleep_cancellable(Duration::from_millis(500), &shutdown).await.is_break() {
                            return;
                        }
                        if let Some(l) = connect_listener(&self.database_url, &shutdown).await {
                            listener = Some(l);
                        }
                        continue;
                    }
                },
                _ = tick.tick() => {
                    // Interval fallback — make sure listener is alive too.
                    if listener.is_none()
                        && let Some(l) = connect_listener(&self.database_url, &shutdown).await
                    {
                        info!("PgListener reconnected on interval tick");
                        listener = Some(l);
                    }
                }
            }

            // Drain up to a few claims per wakeup. Prevents a single
            // NOTIFY from leaving siblings stuck behind the next tick
            // when the backlog is deep.
            for _ in 0..16 {
                if shutdown.is_cancelled() {
                    return;
                }
                match self.try_claim_and_process(&shutdown).await {
                    ClaimResult::Processed => continue,
                    ClaimResult::Backoff => break,
                    ClaimResult::Empty => break,
                    ClaimResult::DbError(msg) => {
                        warn!(error = %msg, "claim cycle failed; will retry");
                        break;
                    }
                }
            }

            // Runtime re-reconcile for `submitted` rows — rows that the
            // executor left for later because the receipt drain timed
            // out, or the RPC was flaky at reconcile time. Without this
            // sweep, a `LeftSubmitted` would be stuck until the process
            // restarted (the boot sweep is the only other re-reconcile
            // path). Plan §8.2 says "re-reconciled on the next poll
            // tick"; this is that.
            //
            // We process at most one row per tick so a pathological
            // backlog doesn't starve the pending queue. Rows with
            // `tx_hash IS NULL` (the pre-submit window) are skipped by
            // `oldest_submitted_with_hash`; boot-sweep handles those
            // after a crash.
            if !shutdown.is_cancelled() {
                self.try_reconcile_one_submitted().await;
            }
        }
    }

    /// Pull one submitted row (oldest `updated_at`, tx_hash present),
    /// run §6.1 reconciliation, and apply the outcome. Transient errors
    /// get a `warn!` — the next tick retries.
    async fn try_reconcile_one_submitted(&self) {
        let row = match db::oldest_submitted_with_hash(&self.pool).await {
            Ok(Some(r)) => r,
            Ok(None) => return, // nothing to reconcile
            Err(e) => {
                warn!(error = %e, "runtime reconcile: list query failed; will retry");
                return;
            }
        };

        let hash = match &row.tx_hash {
            Some(h) => match alloy::primitives::TxHash::from_str(h) {
                Ok(h) => h,
                Err(e) => {
                    error!(
                        tweet_id = %row.tweet_id,
                        raw = %h,
                        error = %e,
                        "runtime reconcile: malformed tx_hash — marking rejected"
                    );
                    let _ = with_retry("runtime_reconcile_mark_rejected_bad_hash", || async {
                        db::mark_rejected(&self.pool, &row.tweet_id, "InvalidTxHash")
                            .await
                            .map_err(Into::into)
                    })
                    .await;
                    return;
                }
            },
            // Should be filtered out by oldest_submitted_with_hash, but
            // be explicit so a future query change doesn't silently skip
            // the branch.
            None => return,
        };

        let outcome = crate::services::gift::reconcile::reconcile_labeled(
            &self.chain,
            self.gift_vault,
            row.token_id,
            row.receiver_id,
            Some(hash),
        )
        .await;

        match outcome {
            crate::services::gift::reconcile::ReconcileOutcome::Completed => {
                info!(tweet_id = %row.tweet_id, %hash, "runtime reconcile: completed");
                let _ = with_retry("runtime_reconcile_mark_completed", || async {
                    db::mark_completed(&self.pool, &row.tweet_id, self.reply_enabled)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            crate::services::gift::reconcile::ReconcileOutcome::Rejected { reason } => {
                info!(
                    tweet_id = %row.tweet_id,
                    %hash,
                    reason = %reason,
                    "runtime reconcile: rejected"
                );
                let _ = with_retry("runtime_reconcile_mark_rejected", || async {
                    db::mark_rejected(&self.pool, &row.tweet_id, &reason)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            crate::services::gift::reconcile::ReconcileOutcome::ResetPending { hint } => {
                warn!(
                    tweet_id = %row.tweet_id,
                    %hash,
                    hint = %hint,
                    "runtime reconcile: tx dropped, resetting to pending"
                );
                let _ = with_retry("runtime_reconcile_reset_pending", || async {
                    db::reset_to_pending(&self.pool, &row.tweet_id, &hint)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            crate::services::gift::reconcile::ReconcileOutcome::KeepSubmitted => {
                debug!(
                    tweet_id = %row.tweet_id,
                    %hash,
                    "runtime reconcile: tx still in mempool, keeping submitted"
                );
                // Touch updated_at so a busy backlog doesn't get stuck
                // re-checking the same row every tick. `set_last_error`
                // with a no-op hint is the cheapest update.
                let _ = with_retry("runtime_reconcile_touch", || async {
                    db::set_last_error(&self.pool, &row.tweet_id, "still_pending_in_mempool")
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            crate::services::gift::reconcile::ReconcileOutcome::Transient(err) => {
                warn!(
                    tweet_id = %row.tweet_id,
                    %hash,
                    error = %err,
                    "runtime reconcile: transient RPC error, will retry next tick"
                );
            }
        }
    }

    /// One claim → preflight → DB-update-or-executor cycle. Runs inside
    /// a single transaction for the lock-and-update steps; hands off to
    /// the executor outside the txn so on-chain waits don't hold a row
    /// lock open for tens of seconds.
    async fn try_claim_and_process(&self, shutdown: &CancellationToken) -> ClaimResult {
        let mut tx = match self.pool.begin().await {
            Ok(t) => t,
            Err(e) => return ClaimResult::DbError(e.to_string()),
        };

        let claim = match db::claim_pending(&mut tx).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                let _ = tx.commit().await;
                return ClaimResult::Empty;
            }
            Err(e) => {
                // DB read error on claim_pending.
                crate::metrics::METRICS.gift.inc_db_read_error();
                let _ = tx.rollback().await;
                return ClaimResult::DbError(e.to_string());
            }
        };

        debug!(tweet_id = %claim.tweet_id, "claimed row");

        // Optional fallback-wait window. Zero on the primary consumer
        // (no-op fast path). On a fallback instance, sleep so the
        // primary has time to land its `setReceiver` tx; the preflight
        // below then sees on-chain receiver == target and short-circuits
        // via `AlreadyBound`. The FOR UPDATE lock from `claim_pending`
        // is held across the sleep — that's intentional: no other
        // consumer should grab this row mid-wait.
        if !self.consumer_wait_time.is_zero() {
            debug!(
                tweet_id = %claim.tweet_id,
                wait_ms = self.consumer_wait_time.as_millis() as u64,
                "fallback wait before preflight"
            );
            if sleep_cancellable(self.consumer_wait_time, shutdown)
                .await
                .is_break()
            {
                // Shutdown during wait — release the lock cleanly and
                // let the next consumer instance retry on next boot.
                let _ = tx.rollback().await;
                return ClaimResult::Empty;
            }
        }

        // --- Preflight ---
        let outcome = preflight(
            &self.chain,
            self.gift_vault,
            claim.token_id,
            claim.receiver_id,
            &claim.handle,
        )
        .await;

        match outcome {
            PreflightOutcome::Rejected(reason) => {
                let update_result = sqlx::query(
                    r#"
                    UPDATE gift_tweet
                       SET status = 'rejected',
                           reject_reason = $2,
                           updated_at = NOW()
                     WHERE tweet_id = $1
                    "#,
                )
                .bind(&claim.tweet_id)
                .bind(reason.as_str())
                .execute(&mut *tx)
                .await;

                match update_result {
                    Ok(_) => {
                        if let Err(e) = tx.commit().await {
                            return ClaimResult::DbError(format!("commit reject: {e}"));
                        }
                        info!(
                            tweet_id = %claim.tweet_id,
                            reason = %reason,
                            "row rejected by preflight"
                        );
                        ClaimResult::Processed
                    }
                    Err(e) => {
                        let _ = tx.rollback().await;
                        ClaimResult::DbError(format!("update reject: {e}"))
                    }
                }
            }
            PreflightOutcome::Transient(err) => {
                let update_result = sqlx::query(
                    r#"
                    UPDATE gift_tweet
                       SET last_error = $2,
                           updated_at = NOW()
                     WHERE tweet_id = $1
                    "#,
                )
                .bind(&claim.tweet_id)
                .bind(&err)
                .execute(&mut *tx)
                .await;
                if let Err(e) = update_result {
                    let _ = tx.rollback().await;
                    return ClaimResult::DbError(format!("update last_error: {e}"));
                }
                if let Err(e) = tx.commit().await {
                    return ClaimResult::DbError(format!("commit transient: {e}"));
                }
                warn!(
                    tweet_id = %claim.tweet_id,
                    error = %err,
                    "preflight transient; row stays pending"
                );
                ClaimResult::Backoff
            }
            PreflightOutcome::Send => {
                // Release the row lock before the long-running submit —
                // another consumer replica shouldn't see this row as
                // pending (executor will transition it to `submitted`
                // very quickly), and we don't want to hold a txn open
                // through a receipt wait.
                //
                // We accept a small window where the process dies
                // between commit and `tx.send()` — that leaves the row
                // as `pending` with no tx_hash, which is safe: the next
                // cycle re-signs. §6.1 nonce-policy covers the case
                // where the stale signed tx is still in mempool.
                if let Err(e) = tx.commit().await {
                    return ClaimResult::DbError(format!("commit send-claim: {e}"));
                }

                match self
                    .executor
                    .submit(&claim.tweet_id, claim.token_id, claim.receiver_id, shutdown)
                    .await
                {
                    Ok(outcome) => {
                        self.apply_execution_outcome(&claim.tweet_id, outcome).await;
                        ClaimResult::Processed
                    }
                    Err(e) => {
                        // DB write for `submitted` failed or shutdown
                        // fired pre-submit. Log and continue; row stays
                        // pending (nothing hit the wire).
                        warn!(
                            tweet_id = %claim.tweet_id,
                            error = %e,
                            "executor pre-submit error; row stays pending"
                        );
                        ClaimResult::Backoff
                    }
                }
            }
        }
    }

    /// Apply the executor's outcome to the DB. Each branch is a single
    /// UPDATE wrapped in `with_retry` so transient DB blips don't turn
    /// a completed tx into a stuck row.
    async fn apply_execution_outcome(&self, tweet_id: &str, outcome: ExecutionOutcome) {
        match outcome {
            ExecutionOutcome::DryRunSkipped => {
                // DRY_RUN is a test/debug mode. Mark the row rejected
                // with a dedicated reason so operators see it didn't
                // actually go on-chain.
                let _ = with_retry("dry_run_mark_rejected", || async {
                    db::mark_rejected(&self.pool, tweet_id, "DryRun")
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            ExecutionOutcome::Completed { tx_hash } => {
                info!(tweet_id, %tx_hash, "row completed");
                crate::metrics::METRICS.gift.record_tx_success();
                let _ = with_retry("mark_completed", || async {
                    db::mark_completed(&self.pool, tweet_id, self.reply_enabled)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            ExecutionOutcome::Rejected { reason, tx_hash } => {
                info!(tweet_id, reason = %reason, ?tx_hash, "row rejected by reconcile");
                // A reconcile reject following an on-chain tx attempt is a
                // tx failure (typical case: tx reverted). Preflight rejects
                // (AlreadyBound / NotConfigured / etc) come through a
                // different branch in `try_claim_and_process` and never
                // produce an `ExecutionOutcome::Rejected`, so this counter
                // tracks real chain-side failures only.
                if tx_hash.is_some() {
                    crate::metrics::METRICS.gift.record_tx_failure();
                }
                let _ = with_retry("mark_rejected", || async {
                    db::mark_rejected(&self.pool, tweet_id, &reason)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            ExecutionOutcome::ResetPending { hint } => {
                warn!(tweet_id, hint = %hint, "resetting row to pending");
                let _ = with_retry("reset_to_pending", || async {
                    db::reset_to_pending(&self.pool, tweet_id, &hint)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
            ExecutionOutcome::LeftSubmitted { tx_hash } => {
                warn!(
                    tweet_id,
                    %tx_hash,
                    "row left as submitted; boot sweep or next tick will reconcile"
                );
            }
            ExecutionOutcome::TransientBeforeSubmit { error } => {
                warn!(tweet_id, error = %error, "pre-submit transient; row stays pending");
                // 사용자 의도: "tx 실패하면 계속 실패 카운트". 제출조차 못 한
                // 케이스도 결국 setReceiver가 안 나간 것이므로 streak에 포함.
                // 한 번 풀려서 Completed가 나오면 자동으로 0으로 리셋된다.
                crate::metrics::METRICS.gift.record_tx_failure();
                let _ = with_retry("set_last_error", || async {
                    db::set_last_error(&self.pool, tweet_id, &error)
                        .await
                        .map_err(Into::into)
                })
                .await;
            }
        }
        // Silence the "unused constant" warning if `REVERTED_REASON`
        // never surfaces via this branch; the enum variant carries the
        // label directly.
        let _ = REVERTED_REASON;
    }
}

async fn connect_listener(url: &str, shutdown: &CancellationToken) -> Option<PgListener> {
    // Race against shutdown so SIGTERM during a slow DNS / TCP handshake
    // doesn't block the main loop.
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => None,
        r = PgListener::connect(url) => match r {
            Ok(mut l) => {
                if let Err(e) = l.listen(NOTIFY_CHANNEL).await {
                    error!(channel = NOTIFY_CHANNEL, error = %e, "LISTEN failed");
                    return None;
                }
                info!(channel = NOTIFY_CHANNEL, "PgListener connected and listening");
                Some(l)
            }
            Err(e) => {
                warn!(error = %e, "PgListener connect failed");
                None
            }
        }
    }
}

/// Sleep that aborts early on shutdown. Returns `Break` when shutdown
/// fires; callers should use `.is_break()` to exit their loop.
async fn sleep_cancellable(d: Duration, shutdown: &CancellationToken) -> std::ops::ControlFlow<()> {
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => std::ops::ControlFlow::Break(()),
        _ = tokio::time::sleep(d) => std::ops::ControlFlow::Continue(()),
    }
}
