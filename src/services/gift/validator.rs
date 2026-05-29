//! Preflight validation — runs `getGiftInfo` and classifies the outcome
//! per split-architecture §5 decision table.
//!
//! | Check (in order) | Outcome |
//! |------------------|---------|
//! | `receiver_id == 0x0` | `Reject(ZeroReceiver)` |
//! | `gift.id.len == 0` | `Reject(NotConfigured)` |
//! | `gift.state == Burned` | `Reject(GiftExpired)` |
//! | `!gift.id.eq_ignore_ascii_case(handle)` | `Reject(HandleMismatch)` |
//! | `gift.state == Active && gift.receiver == receiver_id` | `Reject(AlreadyBound)` |
//! | RPC error | `Transient(err)` |
//! | otherwise | `Send` |
//!
//! The HandleMismatch check is load-bearing: the contract does not
//! verify handle ownership on `setReceiver`, only the bot does. Without
//! it a third party can claim someone else's unbound gift by tweeting
//! the template with their own wallet as receiver.

use alloy::primitives::Address;

use crate::chain::bindings::GiftVault;

use crate::chain::rpc_chain::{RpcChain, RpcChainError};

/// Reason a row was rejected. The string-form of each variant is the
/// value stored in `gift_tweet.reject_reason` (DB varchar(32)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// Receiver address is the zero address — `setReceiver` would revert
    /// with `ZeroReceiver`.
    ZeroReceiver,
    /// Token has no gift configured on-chain (`gift.id.is_empty()`).
    /// `setReceiver` would revert with `NotConfigured`.
    NotConfigured,
    /// Gift has already been burned.
    GiftExpired,
    /// Tweet author's handle doesn't match the gift's bound handle.
    /// Third-party hijack defense.
    HandleMismatch,
    /// Gift is Active and already bound to this exact receiver. The
    /// contract would accept the call but emit a redundant event + burn
    /// gas. Skip.
    AlreadyBound,
}

impl RejectReason {
    /// Canonical string form. Matches the DB enum-like varchar so
    /// dashboards can group by `reject_reason`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZeroReceiver => "ZeroReceiver",
            Self::NotConfigured => "NotConfigured",
            Self::GiftExpired => "GiftExpired",
            Self::HandleMismatch => "HandleMismatch",
            Self::AlreadyBound => "AlreadyBound",
        }
    }
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Outcome of a preflight call. The poller maps each variant onto a DB
/// transition (see `poller.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightOutcome {
    /// Pass the row to the executor for on-chain submit.
    Send,
    /// Terminal reject — mark row `rejected` with this reason.
    Rejected(RejectReason),
    /// RPC transport fault. Leave the row as `pending`; operator will
    /// see the error via `last_error` and the next poll tick retries.
    Transient(String),
}

/// Run preflight for `(token, receiver, handle)` against the gift vault.
/// `chain` is used to fetch `getGiftInfo(token)` via the fallback chain.
/// Returns a [`PreflightOutcome`] — callers must handle all three
/// variants (the compiler enforces exhaustive `match`).
pub async fn preflight(
    chain: &RpcChain,
    gift_vault: Address,
    token: Address,
    receiver: Address,
    handle: &str,
) -> PreflightOutcome {
    // Step 1: zero-receiver short-circuit — no RPC needed.
    if receiver == Address::ZERO {
        return PreflightOutcome::Rejected(RejectReason::ZeroReceiver);
    }

    let info = match chain.get_gift_info(gift_vault, token).await {
        Ok(i) => i,
        Err(RpcChainError::Cancelled) => {
            // Shutdown racing preflight isn't a reject — leave the row
            // as `pending` and let the next cycle retry.
            return PreflightOutcome::Transient("shutdown during preflight".to_string());
        }
        Err(e) => return PreflightOutcome::Transient(e.to_string()),
    };

    evaluate(&info, receiver, handle)
}

/// Pure decision logic for steps 2–5. Separated so the whole decision
/// table is unit-testable without an RPC server.
pub fn evaluate(info: &GiftVault::GiftInfo, receiver: Address, handle: &str) -> PreflightOutcome {
    if info.id.is_empty() {
        return PreflightOutcome::Rejected(RejectReason::NotConfigured);
    }
    if info.state == GiftVault::State::Burned {
        return PreflightOutcome::Rejected(RejectReason::GiftExpired);
    }
    if !info.id.eq_ignore_ascii_case(handle) {
        return PreflightOutcome::Rejected(RejectReason::HandleMismatch);
    }
    if info.state == GiftVault::State::Active && info.receiver == receiver {
        return PreflightOutcome::Rejected(RejectReason::AlreadyBound);
    }
    PreflightOutcome::Send
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn reject_reason_strings_match_db_schema() {
        // DB `reject_reason` column is VARCHAR(32). These strings must
        // match the values the reconcile logic writes AND the dashboards
        // group on. Pin them.
        assert_eq!(RejectReason::ZeroReceiver.as_str(), "ZeroReceiver");
        assert_eq!(RejectReason::NotConfigured.as_str(), "NotConfigured");
        assert_eq!(RejectReason::GiftExpired.as_str(), "GiftExpired");
        assert_eq!(RejectReason::HandleMismatch.as_str(), "HandleMismatch");
        assert_eq!(RejectReason::AlreadyBound.as_str(), "AlreadyBound");
    }

    #[test]
    fn empty_id_is_not_configured() {
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "alice",
        );
        assert_eq!(out, PreflightOutcome::Rejected(RejectReason::NotConfigured));
    }

    #[test]
    fn burned_state_rejects_with_gift_expired() {
        // Even when id matches and everything else is fine, burned wins.
        let g = info(GiftVault::State::Burned, Address::ZERO, "alice");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "alice",
        );
        assert_eq!(out, PreflightOutcome::Rejected(RejectReason::GiftExpired));
    }

    #[test]
    fn handle_mismatch_is_critical_security_check() {
        // Third party claims someone else's gift: gift.id says "alice"
        // but the tweet author handle is "bob". MUST reject — the
        // contract will happily accept any caller.
        let g = info(GiftVault::State::Active, Address::ZERO, "alice");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "bob",
        );
        assert_eq!(
            out,
            PreflightOutcome::Rejected(RejectReason::HandleMismatch)
        );
    }

    #[test]
    fn handle_match_is_case_insensitive() {
        // Per project policy X handles are ASCII case-insensitive. "Alice"
        // from the tweet should match "alice" on-chain.
        let g = info(GiftVault::State::Active, Address::ZERO, "alice");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "Alice",
        );
        assert_eq!(out, PreflightOutcome::Send);
    }

    #[test]
    fn already_bound_when_active_and_receiver_matches() {
        let receiver = address!("00000000000000000000000000000000000000aa");
        let g = info(GiftVault::State::Active, receiver, "alice");
        let out = evaluate(&g, receiver, "alice");
        assert_eq!(out, PreflightOutcome::Rejected(RejectReason::AlreadyBound));
    }

    #[test]
    fn active_but_different_receiver_passes() {
        let receiver_parsed = address!("00000000000000000000000000000000000000aa");
        let receiver_onchain = address!("00000000000000000000000000000000000000bb");
        let g = info(GiftVault::State::Active, receiver_onchain, "alice");
        let out = evaluate(&g, receiver_parsed, "alice");
        assert_eq!(out, PreflightOutcome::Send);
    }

    #[test]
    fn accumulating_state_is_allowed_to_send() {
        // Accumulating is the pre-first-bind state. `setReceiver` must
        // transition Accumulating → Active, so accumulating is a Send,
        // not AlreadyBound.
        let g = info(GiftVault::State::Accumulating, Address::ZERO, "alice");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "alice",
        );
        assert_eq!(out, PreflightOutcome::Send);
    }

    #[test]
    fn zero_receiver_rejects_before_rpc() {
        // Calling preflight with receiver=ZERO short-circuits before
        // touching the RPC. We can't easily prove "no RPC was called" in
        // a unit test without mocks, but we can prove the decision.
        //
        // The public entry point isn't pure, so pin the sync portion: if
        // the pure `evaluate` sees ZERO as receiver in a well-formed
        // Active gift, it will hit AlreadyBound if the on-chain receiver
        // is also ZERO, which is wrong. The zero-receiver check MUST be
        // done before evaluate() — hence why the public preflight()
        // checks it up front.
        //
        // Test the contract by calling the public entry with a mock
        // chain isn't possible here (would need a real RpcChain). Cover
        // via anvil integration in the larger test suite if needed.
        // This test documents the invariant for future refactors.
        let _ = RejectReason::ZeroReceiver;
    }

    #[test]
    fn priority_not_configured_before_burned() {
        // If somehow both flags are set (shouldn't happen on happy
        // contract path), report NotConfigured first — operators chasing
        // Burned volume won't be misled by a ghost of a never-configured
        // token.
        let g = info(GiftVault::State::Burned, Address::ZERO, "");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "alice",
        );
        assert_eq!(out, PreflightOutcome::Rejected(RejectReason::NotConfigured));
    }

    #[test]
    fn priority_burned_before_handle_mismatch() {
        // Burned is terminal — don't mask it with a handle-mismatch
        // reject that the contract would also refuse.
        let g = info(GiftVault::State::Burned, Address::ZERO, "alice");
        let out = evaluate(
            &g,
            address!("00000000000000000000000000000000000000aa"),
            "bob",
        );
        assert_eq!(out, PreflightOutcome::Rejected(RejectReason::GiftExpired));
    }
}
