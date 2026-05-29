//! Executor — submits `setReceiver` for a validated row and then
//! reconciles against on-chain state (§6.1). The receipt is a hint;
//! `getGiftInfo` is the source of truth.
//!
//! Contract with the poller:
//! - Input: already-preflighted `(token, receiver)` plus the row's
//!   `tweet_id` (for DB writes).
//! - Output: an [`ExecutionOutcome`] that tells the poller which DB
//!   transition to perform.
//!
//! The executor owns the DB writes that depend on transient state
//! (recording `submitted` + tx_hash), since doing it here is the only
//! way to guarantee the tx_hash is durable before we await the receipt.
//! The poller owns the final transition (completed / rejected / reset
//! to pending) after reconciliation.

use std::time::{Duration, Instant};

use alloy::primitives::{Address, TxHash};
use alloy::rpc::types::TransactionReceipt;
use alloy::sol_types::SolCall;
use crate::chain::bindings::GiftVault;
use crate::services::gift::db_retry::with_retry;
use sqlx::Error as SqlxError;
use sqlx::PgPool;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

use crate::services::gift::db;
use crate::services::gift::reconcile::{ReconcileOutcome, reconcile_labeled};
use crate::chain::rpc_chain::{RpcChain, RpcChainError};
use crate::services::gift::validator::RejectReason;

/// Knobs the poller projects out of `ConsumerConfig`.
#[derive(Debug, Clone, Copy)]
pub struct ExecutorConfig {
    pub dry_run: bool,
    /// Confirmations to wait for before treating the tx as final.
    /// Currently unused — the new chain-failover send path waits for a
    /// receipt via polling, with no per-block confirmation count. The
    /// field is preserved so we can wire it back when a multi-block
    /// reorg policy is needed; today's deployment uses
    /// `TX_CONFIRMATIONS=1` (default) and the receipt-or-state-truth
    /// pattern handles single-confirmation finality.
    #[allow(dead_code)]
    pub tx_confirmations: u64,
    pub tx_graceful_drain_ms: u64,
    pub gift_vault: Address,
    /// Whether the consumer should mark completed rows as `reply_status='pending'`
    /// for the reply worker. Mirrors `ConsumerConfig::reply.is_some()` —
    /// `false` means the reply feature is disabled and completed rows stay
    /// at `reply_status='none'`. Lifting this into the executor avoids
    /// threading the full `ReplyConfig` through every DB call.
    pub reply_enabled: bool,
}

/// Outcome of `submit`. The poller uses this to decide the final DB
/// transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// `DRY_RUN=true` — pretend success without touching the chain.
    DryRunSkipped,
    /// `setReceiver` succeeded (receipt status=1, reconciliation agrees).
    Completed { tx_hash: TxHash },
    /// On-chain state says the row should be marked rejected. Carries
    /// the DB-facing `reject_reason` string.
    Rejected {
        reason: String,
        tx_hash: Option<TxHash>,
    },
    /// Tx was submitted but reconciliation says it's dropped — reset to
    /// pending so the next cycle re-signs with a fresh nonce.
    ResetPending { hint: String },
    /// Receipt wait hit the shutdown drain window OR reconcile says the
    /// tx is still pending in mempool. Leave row as `submitted`; boot
    /// sweep or next poll tick will reconcile again.
    LeftSubmitted { tx_hash: TxHash },
    /// Transient failure before the tx hit the wire (nonce lookup / fee
    /// estimate / raw send on every endpoint). Row stays `pending`,
    /// `last_error` gets the message.
    TransientBeforeSubmit { error: String },
}

/// Error surfaced by `submit` for cases where we couldn't even record a
/// `submitted` transition. The poller logs these and moves on.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("shutdown signaled before tx was submitted")]
    Cancelled,

    #[error("db write failed while recording submitted: {0}")]
    DbSubmitted(SqlxError),
}

pub struct Executor {
    chain: RpcChain,
    pool: PgPool,
    config: ExecutorConfig,
}

impl Executor {
    pub fn new(chain: RpcChain, pool: PgPool, config: ExecutorConfig) -> Self {
        Self {
            chain,
            pool,
            config,
        }
    }

    /// Submit `setReceiver(token, receiver)` for the given row.
    ///
    /// Flow:
    /// 1. Build + sign + send raw tx (non-cancellable once on the wire).
    /// 2. On accept, persist `tx_hash` + `status='submitted'` BEFORE
    ///    awaiting the receipt — if the process dies mid-wait, boot
    ///    reconciliation picks up the row.
    /// 3. Await the receipt, but respect a shutdown drain window.
    /// 4. Always reconcile via `getGiftInfo` after the wait, regardless
    ///    of receipt outcome. State is authoritative.
    #[instrument(
        name = "executor.submit",
        skip_all,
        fields(
            tweet_id = tweet_id,
            token = %token,
            receiver = %receiver,
        )
    )]
    pub async fn submit(
        &self,
        tweet_id: &str,
        token: Address,
        receiver: Address,
        shutdown: &CancellationToken,
    ) -> Result<ExecutionOutcome, ExecutorError> {
        if self.config.dry_run {
            warn!("DRY_RUN=true — skipping on-chain submit");
            return Ok(ExecutionOutcome::DryRunSkipped);
        }

        if shutdown.is_cancelled() {
            return Err(ExecutorError::Cancelled);
        }

        // --- Nonce + fee lookup via fallback chain ---
        let nonce = match self.chain.get_nonce_pending().await {
            Ok(n) => n,
            Err(RpcChainError::Cancelled) => return Err(ExecutorError::Cancelled),
            Err(e) => {
                warn!(error = %e, "nonce lookup failed on all endpoints");
                return Ok(ExecutionOutcome::TransientBeforeSubmit {
                    error: e.to_string(),
                });
            }
        };
        let fees = match self.chain.estimate_fees().await {
            Ok(f) => f,
            Err(RpcChainError::Cancelled) => return Err(ExecutorError::Cancelled),
            Err(e) => {
                warn!(error = %e, "fee estimate failed on all endpoints");
                return Ok(ExecutionOutcome::TransientBeforeSubmit {
                    error: e.to_string(),
                });
            }
        };

        // --- Sign once + submit across the failover chain ---
        // Sign on the primary, then replay the same raw envelope across
        // every endpoint until one accepts. Monad nodes share the
        // mempool, so `AlreadyKnown` from a sub = success. We never
        // race `send` against shutdown — once a node accepts the wire
        // bytes, the hash is ours.
        let bot = self.chain.signer_address();
        let tx_hash = match self
            .chain
            .send_setreceiver(self.config.gift_vault, token, receiver, nonce, fees, bot)
            .await
        {
            Ok(h) => h,
            Err(RpcChainError::Cancelled) => return Err(ExecutorError::Cancelled),
            Err(e) => {
                let msg = match &e {
                    RpcChainError::AllEndpointsFailed { reason, .. } => reason.clone(),
                    other => other.to_string(),
                };
                let class = classify(&msg);
                warn!(error = %msg, class = ?class, "send_setreceiver failed across chain");
                return Ok(match class {
                    // Node rejected the call before it hit the mempool —
                    // typical causes: preflight revert, contract check
                    // we forgot to mirror in `validator`, state race.
                    // Same outcome on every endpoint, so failover is
                    // pointless; mark the row rejected.
                    ErrorClass::Reverted => ExecutionOutcome::Rejected {
                        reason: "Reverted".to_string(),
                        tx_hash: None,
                    },
                    // Bot EOA out of funds — retry after operator tops
                    // it up. Row stays pending; last_error surfaces the
                    // balance state to dashboards.
                    ErrorClass::InsufficientFunds => {
                        error!(error = %msg, "bot EOA insufficient funds");
                        ExecutionOutcome::TransientBeforeSubmit { error: msg }
                    }
                    // Re-fetch nonce on next poll; no backoff for a
                    // state-drift race.
                    ErrorClass::NonceTooLow | ErrorClass::Transient => {
                        ExecutionOutcome::TransientBeforeSubmit { error: msg }
                    }
                });
            }
        };
        info!(%tx_hash, nonce, "setReceiver submitted via RpcChain");

        // Persist tx_hash BEFORE awaiting receipt so a mid-wait crash
        // leaves a trail for boot reconciliation.
        let hash_hex = format!("{tx_hash:#x}");
        if let Err(e) = with_retry("mark_submitted", || async {
            db::mark_submitted(&self.pool, tweet_id, &hash_hex)
                .await
                .map_err(|db_err| match db_err {
                    db::DbError::Sqlx(sqlx_err) => sqlx_err,
                    other => sqlx::Error::Protocol(other.to_string()),
                })
        })
        .await
        {
            error!(error = %e, "failed to record submitted status in DB");
            return Err(ExecutorError::DbSubmitted(e));
        }

        // --- Receipt wait with shutdown drain ---
        // Poll get_receipt across the chain (fails over for read).
        // Returns when:
        //   - receipt is found (Some(receipt))
        //   - shutdown fires AND TX_GRACEFUL_DRAIN_MS expires
        //   - chain read errors (we treat as "no receipt yet, reconcile anyway")
        let drain = Duration::from_millis(self.config.tx_graceful_drain_ms);
        let receipt_outcome = wait_for_receipt(&self.chain, tx_hash, shutdown, drain).await;
        match receipt_outcome {
            ReceiptWaitOutcome::Confirmed(receipt) => {
                if receipt.status() {
                    info!(%tx_hash, "receipt status=1; reconciling to confirm semantics");
                } else {
                    warn!(
                        %tx_hash,
                        block = ?receipt.block_number,
                        "receipt status=0 (reverted); reconciling against on-chain state"
                    );
                }
            }
            ReceiptWaitOutcome::DrainTimeout => {
                warn!(%tx_hash, "graceful drain timeout — leaving row as submitted");
                return Ok(ExecutionOutcome::LeftSubmitted { tx_hash });
            }
            ReceiptWaitOutcome::FetchFailed(reason) => {
                // Read couldn't complete on any endpoint — don't trust
                // anything, just reconcile against on-chain state.
                warn!(error = %reason, %tx_hash, "receipt fetch failed; falling through to reconcile");
            }
        }

        let outcome = reconcile_labeled(
            &self.chain,
            self.config.gift_vault,
            token,
            receiver,
            Some(tx_hash),
        )
        .await;

        match outcome {
            ReconcileOutcome::Completed => Ok(ExecutionOutcome::Completed { tx_hash }),
            ReconcileOutcome::Rejected { reason } => Ok(ExecutionOutcome::Rejected {
                reason,
                tx_hash: Some(tx_hash),
            }),
            ReconcileOutcome::ResetPending { hint } => Ok(ExecutionOutcome::ResetPending { hint }),
            ReconcileOutcome::KeepSubmitted => Ok(ExecutionOutcome::LeftSubmitted { tx_hash }),
            ReconcileOutcome::Transient(err) => {
                warn!(error = %err, %tx_hash, "reconciliation hit a transient error; leaving row submitted");
                Ok(ExecutionOutcome::LeftSubmitted { tx_hash })
            }
        }
    }

    /// Boot-time reconcile helper. Wraps `reconcile_labeled` + DB write
    /// so the main.rs sweep loop is a one-liner.
    pub async fn reconcile_submitted_row(
        &self,
        row: &db::GiftRow,
    ) -> Result<ReconcileOutcome, SqlxError> {
        let tx_hash = row
            .tx_hash
            .as_deref()
            .and_then(|s| s.parse::<TxHash>().ok());
        let outcome = reconcile_labeled(
            &self.chain,
            self.config.gift_vault,
            row.token_id,
            row.receiver_id,
            tx_hash,
        )
        .await;

        match &outcome {
            ReconcileOutcome::Completed => {
                with_retry("mark_completed", || async {
                    db::mark_completed(&self.pool, &row.tweet_id, self.config.reply_enabled)
                        .await
                        .map_err(Into::into)
                })
                .await?;
            }
            ReconcileOutcome::Rejected { reason } => {
                with_retry("mark_rejected", || async {
                    db::mark_rejected(&self.pool, &row.tweet_id, reason)
                        .await
                        .map_err(Into::into)
                })
                .await?;
            }
            ReconcileOutcome::ResetPending { hint } => {
                with_retry("reset_to_pending", || async {
                    db::reset_to_pending(&self.pool, &row.tweet_id, hint)
                        .await
                        .map_err(Into::into)
                })
                .await?;
            }
            ReconcileOutcome::KeepSubmitted => {
                // Nothing to update — row stays submitted for the next
                // sweep.
            }
            ReconcileOutcome::Transient(err) => {
                with_retry("set_last_error", || async {
                    db::set_last_error(&self.pool, &row.tweet_id, err)
                        .await
                        .map_err(Into::into)
                })
                .await?;
            }
        }

        Ok(outcome)
    }
}

// impl glue so `db::DbError` converts cleanly through `with_retry`'s
// `sqlx::Error` signature.
impl From<db::DbError> for sqlx::Error {
    fn from(e: db::DbError) -> Self {
        match e {
            db::DbError::Sqlx(s) => s,
            other => sqlx::Error::Protocol(other.to_string()),
        }
    }
}

/// Outcome of polling for a receipt. Drives the executor's reconcile
/// step; we always reconcile regardless, but the variant lets us log
/// what we actually saw.
#[derive(Debug)]
enum ReceiptWaitOutcome {
    /// Tx was mined and the receipt was returned. Caller still
    /// reconciles via `getGiftInfo` because state is authoritative.
    /// Boxed because `TransactionReceipt` is ~576 bytes; the rare
    /// "drain timeout" / "fetch failed" variants would otherwise
    /// inflate the whole enum to that size.
    Confirmed(Box<TransactionReceipt>),
    /// Shutdown fired and `TX_GRACEFUL_DRAIN_MS` elapsed without a
    /// receipt. Row stays `submitted` — boot sweep / runtime
    /// re-reconcile will pick it up later.
    DrainTimeout,
    /// Every endpoint errored on `eth_getTransactionReceipt` for the
    /// full poll window. Reconcile anyway — `getGiftInfo` is the
    /// source of truth, receipt was always a hint.
    FetchFailed(String),
}

/// Poll `eth_getTransactionReceipt` across the chain until the tx is
/// mined or shutdown's drain window expires. Replaces alloy's
/// `PendingTransactionBuilder::get_receipt()` because that path is
/// tied to a single endpoint — we want failover for reads too.
async fn wait_for_receipt(
    chain: &RpcChain,
    hash: TxHash,
    shutdown: &CancellationToken,
    drain: Duration,
) -> ReceiptWaitOutcome {
    /// How often we re-query `eth_getTransactionReceipt`. Short enough
    /// that confirmation latency is dominated by chain time, not poll
    /// time; long enough that an idle bot doesn't hammer the RPC.
    const POLL_INTERVAL: Duration = Duration::from_millis(500);

    /// Hard cap on the receipt wait during NORMAL operation (shutdown has
    /// its own `drain` window). Without this, a tx that is accepted but
    /// never mined (dropped/evicted from the mempool) polls forever and
    /// blocks the single-leader consumer from processing any other gift
    /// row until restart. On timeout we fall through to `reconcile_labeled`,
    /// which uses `probe_transaction` to decide safely: still pending →
    /// KeepSubmitted (no resubmit), dropped → ResetPending (resubmit). So
    /// bounding the wait cannot cause a double-submit.
    const MAX_RECEIPT_WAIT: Duration = Duration::from_secs(120);

    let mut shutdown_deadline: Option<Instant> = None;
    let mut last_err: Option<String> = None;
    let normal_deadline = Instant::now() + MAX_RECEIPT_WAIT;

    loop {
        match chain.get_receipt(hash).await {
            Ok(Some(receipt)) => return ReceiptWaitOutcome::Confirmed(Box::new(receipt)),
            Ok(None) => {
                // Not mined yet — fall through to the wait below.
            }
            Err(RpcChainError::Cancelled) => {
                // Shutdown propagated through the read — handle below.
            }
            Err(e) => {
                last_err = Some(e.to_string());
                // Don't bail immediately — chain failover already tried
                // every endpoint. Sleep and re-poll; the network may
                // recover while we wait. If shutdown is active we'll
                // still respect the drain window.
            }
        }

        // Manage the drain window: once shutdown fires, start the
        // countdown. After drain, return DrainTimeout. While the
        // window is open we keep polling.
        if shutdown.is_cancelled() && shutdown_deadline.is_none() {
            shutdown_deadline = Some(Instant::now() + drain);
            warn!(
                %hash,
                drain_ms = drain.as_millis() as u64,
                "shutdown fired mid-confirmation; draining receipt"
            );
        }

        if let Some(deadline) = shutdown_deadline
            && Instant::now() >= deadline
        {
            return match last_err {
                Some(reason) => ReceiptWaitOutcome::FetchFailed(reason),
                None => ReceiptWaitOutcome::DrainTimeout,
            };
        }

        // Normal-operation cap: not shutting down, but the receipt never
        // showed within MAX_RECEIPT_WAIT. Stop waiting and let
        // `reconcile_labeled` (via `probe_transaction`) decide — pending
        // stays submitted, dropped gets re-signed. This unblocks the
        // single-leader consumer instead of hanging on one stuck tx.
        if shutdown_deadline.is_none() && Instant::now() >= normal_deadline {
            warn!(
                %hash,
                max_wait_s = MAX_RECEIPT_WAIT.as_secs(),
                "receipt not observed within max wait; falling through to reconcile"
            );
            return ReceiptWaitOutcome::FetchFailed(
                "receipt not observed within max wait".to_string(),
            );
        }

        // Bound the sleep against shutdown — if shutdown fires mid-
        // sleep we wake immediately and re-evaluate the deadline.
        tokio::select! {
            biased;
            _ = shutdown.cancelled(), if shutdown_deadline.is_none() => {
                // Shutdown just fired — loop will set the deadline next
                // iteration.
            }
            _ = sleep(POLL_INTERVAL) => {}
        }
    }
}

/// Best-effort RPC error classification mirroring `common::pipeline::executor`.
/// Kept local so consumer can evolve independently without touching the
/// shared crate (per the worktree directive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    InsufficientFunds,
    NonceTooLow,
    Reverted,
    Transient,
}

pub fn classify(msg: &str) -> ErrorClass {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("insufficient funds") || lower.contains("insufficient balance") {
        ErrorClass::InsufficientFunds
    } else if lower.contains("nonce too low")
        || lower.contains("nonce is too low")
        || lower.contains("invalid nonce")
        || lower.contains("nonce has already been used")
    {
        ErrorClass::NonceTooLow
    } else if lower.contains("execution reverted")
        || lower.contains("vm exception")
        || lower.contains("0x08c379a0")
        || lower.contains("0x4e487b71")
    {
        ErrorClass::Reverted
    } else {
        ErrorClass::Transient
    }
}

/// Compile-time witness: keep the full `setReceiver` ABI reachable from
/// the executor path even when the inner call builder changes. Prevents
/// a silent loss-of-coverage if alloy restructures its sol! codegen.
#[allow(dead_code)]
fn _setreceiver_witness() {
    let _ = GiftVault::setReceiverCall::SELECTOR;
}

/// Unused — but documents that we understand the non-cancellable
/// contract for `tx.send()`.
#[allow(dead_code)]
fn _reject_transient(_: &RejectReason) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_handles_common_shapes() {
        assert_eq!(
            classify("insufficient funds for gas * price + value"),
            ErrorClass::InsufficientFunds
        );
        assert_eq!(classify("nonce too low"), ErrorClass::NonceTooLow);
        assert_eq!(
            classify("execution reverted: NotConfigured"),
            ErrorClass::Reverted
        );
        assert_eq!(classify("connection refused"), ErrorClass::Transient);
        assert_eq!(
            classify("stream reverted to fallback codec"),
            ErrorClass::Transient,
            "bare 'reverted' must not mask transient transport errors"
        );
    }

    #[test]
    fn classify_is_case_insensitive() {
        assert_eq!(classify("Nonce Too Low"), ErrorClass::NonceTooLow);
        assert_eq!(classify("EXECUTION REVERTED"), ErrorClass::Reverted);
    }

    #[test]
    fn classify_recognizes_solidity_revert_selectors() {
        assert_eq!(
            classify("returned data: 0x08c379a0...."),
            ErrorClass::Reverted,
            "Error(string) selector must classify as reverted"
        );
        assert_eq!(
            classify("panic: 0x4e487b71"),
            ErrorClass::Reverted,
            "Panic(uint256) selector must classify as reverted"
        );
    }
}
