//! On-chain state reconciliation — the split-architecture §6.1 routine.
//!
//! `getGiftInfo(token)` is the **source of truth**. Receipts are only a
//! convenience signal; if we can't fetch one we still know whether a tx
//! took effect by reading state.
//!
//! Used in two places:
//! 1. Runtime — after `tx.send()` + receipt-wait, regardless of whether
//!    the receipt succeeded or failed.
//! 2. Boot — sweep every `status='submitted'` row before entering the
//!    normal pending loop (§8.2). Guarantees we don't double-submit for
//!    rows whose prior tx actually landed.

use alloy::primitives::{Address, TxHash};

use crate::chain::bindings::GiftVault;

use crate::chain::rpc_chain::{RpcChain, RpcChainError, TxProbe};
use crate::services::gift::validator::RejectReason;

/// Canonical reason string stored in `gift_tweet.reject_reason` for a
/// tx that was mined but whose state didn't advance. `RejectReason`
/// doesn't carry a `Reverted` variant (that's a tx-level outcome, not a
/// preflight decision); reconciliation writes this literal string
/// directly.
pub const REVERTED_REASON: &str = "Reverted";

/// Richer reconcile output that includes a string-level reject label so
/// the poller can store `"Reverted"` without bloating `RejectReason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Completed,
    Rejected { reason: String },
    ResetPending { hint: String },
    KeepSubmitted,
    Transient(String),
}

/// Reconcile a single row against on-chain state per §6.1. Returns a
/// [`ReconcileOutcome`] whose reject reason is the string form suitable
/// for direct insertion into `gift_tweet.reject_reason`.
///
/// Inputs:
/// - `chain` — RPC fallback chain.
/// - `gift_vault` — address of the configured `GiftVault`.
/// - `token` — the token the row targets.
/// - `intended_receiver` — the receiver the bot tried to set.
/// - `tx_hash` — optional. `None` is the boot-sweep case for a row that
///   wrote `status='submitted'` but had no `tx_hash` recorded (shouldn't
///   happen on the happy path, but we handle it by using state-only
///   reasoning).
pub async fn reconcile_labeled(
    chain: &RpcChain,
    gift_vault: Address,
    token: Address,
    intended_receiver: Address,
    tx_hash: Option<TxHash>,
) -> ReconcileOutcome {
    // Implemented independently rather than as a wrapper over `reconcile`
    // so the "mined but reverted" branch can write the literal "Reverted"
    // string without re-encoding through the RejectReason enum.
    let info = match chain.get_gift_info(gift_vault, token).await {
        Ok(i) => i,
        Err(RpcChainError::Cancelled) => {
            return ReconcileOutcome::Transient("shutdown during reconcile".to_string());
        }
        Err(e) => return ReconcileOutcome::Transient(e.to_string()),
    };

    if info.state == GiftVault::State::Active && info.receiver == intended_receiver {
        return ReconcileOutcome::Completed;
    }
    if info.state == GiftVault::State::Burned {
        return ReconcileOutcome::Rejected {
            reason: RejectReason::GiftExpired.as_str().to_string(),
        };
    }
    if info.state == GiftVault::State::Active && info.receiver != intended_receiver {
        return ReconcileOutcome::Rejected {
            reason: RejectReason::AlreadyBound.as_str().to_string(),
        };
    }

    let Some(hash) = tx_hash else {
        return ReconcileOutcome::ResetPending {
            hint: "reconcile: submitted row has no tx_hash".to_string(),
        };
    };

    match chain.probe_transaction(hash).await {
        Ok(TxProbe::Unknown) => ReconcileOutcome::ResetPending {
            hint: "tx_dropped_from_mempool".to_string(),
        },
        Ok(TxProbe::PendingInMempool) => ReconcileOutcome::KeepSubmitted,
        Ok(TxProbe::Mined { .. }) => ReconcileOutcome::Rejected {
            reason: REVERTED_REASON.to_string(),
        },
        Err(RpcChainError::Cancelled) => {
            ReconcileOutcome::Transient("shutdown during tx probe".to_string())
        }
        Err(e) => ReconcileOutcome::Transient(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! Decision-table tests for reconciliation. We test the pure branches
    //! by driving a small stub that returns pre-canned `GiftInfo` + tx
    //! probe values without actually running an RPC — the `reconcile_labeled`
    //! function is small enough that we replicate its control flow in a
    //! helper that takes already-fetched state.

    use super::*;
    use alloy::primitives::{U256, address};

    fn info(state: GiftVault::State, receiver: Address, id: &str) -> GiftVault::GiftInfo {
        GiftVault::GiftInfo {
            state,
            platform: GiftVault::Platform::X,
            receiver,
            balance: U256::ZERO,
            createdAt: U256::ZERO,
            id: id.to_string(),
        }
    }

    /// Decision-table driver: same branches as `reconcile_labeled` but
    /// takes pre-fetched `info` + `probe` so we can exercise each branch
    /// without a live RPC.
    fn decide(
        info: &GiftVault::GiftInfo,
        intended_receiver: Address,
        probe: Option<TxProbe>,
    ) -> ReconcileOutcome {
        if info.state == GiftVault::State::Active && info.receiver == intended_receiver {
            return ReconcileOutcome::Completed;
        }
        if info.state == GiftVault::State::Burned {
            return ReconcileOutcome::Rejected {
                reason: RejectReason::GiftExpired.as_str().to_string(),
            };
        }
        if info.state == GiftVault::State::Active && info.receiver != intended_receiver {
            return ReconcileOutcome::Rejected {
                reason: RejectReason::AlreadyBound.as_str().to_string(),
            };
        }
        match probe {
            None => ReconcileOutcome::ResetPending {
                hint: "reconcile: submitted row has no tx_hash".to_string(),
            },
            Some(TxProbe::Unknown) => ReconcileOutcome::ResetPending {
                hint: "tx_dropped_from_mempool".to_string(),
            },
            Some(TxProbe::PendingInMempool) => ReconcileOutcome::KeepSubmitted,
            Some(TxProbe::Mined { .. }) => ReconcileOutcome::Rejected {
                reason: REVERTED_REASON.to_string(),
            },
        }
    }

    #[test]
    fn intent_matches_on_chain_state_completes() {
        let receiver = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Active, receiver, "alice");
        assert_eq!(
            decide(&g, receiver, Some(TxProbe::PendingInMempool)),
            ReconcileOutcome::Completed
        );
    }

    #[test]
    fn burned_gift_rejects_with_gift_expired() {
        let intended = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Burned, Address::ZERO, "alice");
        match decide(&g, intended, Some(TxProbe::PendingInMempool)) {
            ReconcileOutcome::Rejected { reason } => assert_eq!(reason, "GiftExpired"),
            other => panic!("expected Rejected(GiftExpired), got {other:?}"),
        }
    }

    #[test]
    fn different_receiver_won_rejects_with_already_bound() {
        // Race: operator or another bot flipped the gift to a different
        // receiver. We MUST NOT overwrite — treat as a terminal reject.
        let intended = address!("00000000000000000000000000000000000000aa");
        let other = address!("00000000000000000000000000000000000000bb");
        let g = info(GiftVault::State::Active, other, "alice");
        match decide(&g, intended, Some(TxProbe::PendingInMempool)) {
            ReconcileOutcome::Rejected { reason } => assert_eq!(reason, "AlreadyBound"),
            other => panic!("expected Rejected(AlreadyBound), got {other:?}"),
        }
    }

    #[test]
    fn dropped_from_mempool_resets_to_pending() {
        let intended = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "alice");
        match decide(&g, intended, Some(TxProbe::Unknown)) {
            ReconcileOutcome::ResetPending { hint } => {
                assert_eq!(hint, "tx_dropped_from_mempool");
            }
            other => panic!("expected ResetPending, got {other:?}"),
        }
    }

    #[test]
    fn still_pending_keeps_submitted() {
        let intended = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "alice");
        assert_eq!(
            decide(&g, intended, Some(TxProbe::PendingInMempool)),
            ReconcileOutcome::KeepSubmitted
        );
    }

    #[test]
    fn mined_but_state_unchanged_rejects_with_reverted() {
        // Tx landed in a block, but on-chain state shows the gift still
        // isn't Active — must have reverted.
        let intended = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "alice");
        match decide(&g, intended, Some(TxProbe::Mined { block_number: 42 })) {
            ReconcileOutcome::Rejected { reason } => assert_eq!(reason, "Reverted"),
            other => panic!("expected Rejected(Reverted), got {other:?}"),
        }
    }

    #[test]
    fn submitted_row_without_tx_hash_resets_to_pending() {
        // Defensive: boot-sweep finds a `submitted` row with NULL
        // tx_hash (e.g. an old row from before the column was written
        // atomically). Reset it so the next cycle re-signs.
        let intended = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "alice");
        match decide(&g, intended, None) {
            ReconcileOutcome::ResetPending { hint } => {
                assert!(hint.contains("no tx_hash"), "unexpected hint: {hint}");
            }
            other => panic!("expected ResetPending, got {other:?}"),
        }
    }
}
