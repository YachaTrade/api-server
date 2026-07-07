//! X (Twitter) hidden-creator verification: orchestration + ownership checks.
//!
//! `reserve()` is the security-critical entry point: a token's `token_id`
//! (CREATE2 address) is precomputed by an UNAUTHENTICATED, client-supplied-
//! `creator` endpoint (`/token/salt`), and once a token deploys, its salt
//! becomes public on-chain (`BondingCurveRouter.create` calldata) — so any
//! check that only proves "the caller knows this salt" is worthless
//! post-deploy. `reserve()` runs the creator check + CREATE2 salt-possession
//! check (see `creator_matches` / `address_matches`), confirms on-chain that
//! the token is NOT yet deployed (fail-closed — the salt is still secret at
//! this point), then records an atomic first-writer-wins reservation binding
//! `token_id → account_id`. `finalize()` is reservation-only: it just checks
//! that a prior `reserve()` for this `token_id` belongs to the calling
//! session, then persists the pending X-verification signals.

pub mod onchain;

use std::str::FromStr;
use std::sync::Arc;

use alloy::primitives::{Address, B256};
use tracing::error;

use crate::config::{X_FOLLOWED_BY_MAX, X_PENDING_TTL_MS};
use crate::controllers::x_verification::XVerificationController;
use crate::db::{postgres::PostgresDatabase, redis::RedisDatabase};
use crate::result::AppError;
use crate::services::token::salt::SaltService;
use crate::services::x_oauth::client;
use crate::types::token::x_verification::XFollowedByEntry;
use crate::types::x_verification::{
    FinalizeRequest, FollowedByResponse, OAuthLoginResponse, ReserveRequest, StatusResponse,
    XOAuthState, XPending, append_followed_by,
};
use crate::utils::valid_account_id;

/// creator (from request) must equal the authenticated session wallet.
/// EIP-55 checksum comparison — never LOWER().
pub fn creator_matches(creator: &str, session_address: &str) -> Result<(), AppError> {
    let creator_cs = valid_account_id(creator)
        .ok_or_else(|| AppError::BadRequest("invalid creator address".into()))?;
    let session_cs = valid_account_id(session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    if creator_cs != session_cs {
        return Err(AppError::Forbidden("creator_mismatch".into()));
    }
    Ok(())
}

/// The recomputed CREATE2 address must equal the client-supplied token_id.
/// Address byte-equality is checksum-agnostic (equivalent to EIP-55 compare).
pub fn address_matches(expected: Address, token_id: &str) -> Result<(), AppError> {
    let claimed =
        Address::from_str(token_id).map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
    if expected != claimed {
        return Err(AppError::Forbidden("creator_mismatch".into()));
    }
    Ok(())
}

/// The identifiers actually persisted after both ownership checks pass:
/// always the EIP-55 checksum form, never the raw client/caller-cased
/// strings. `address_matches` / `creator_matches` only prove *byte*
/// equality (case-agnostic), so a lowercase/mixed-case `token_id` or
/// `session_address` would otherwise be written to Postgres verbatim —
/// silently failing to join against `token.token_id` (written in checksum
/// form by the Observer indexer), and since `ON CONFLICT (token_id)` is
/// case-sensitive, casing variants across re-finalize attempts could even
/// create duplicate rows instead of upserting.
pub fn canonical_persist_ids(
    expected: Address,
    session_address: &str,
) -> Result<(String, String), AppError> {
    let token_id_cs = expected.to_checksum(None);
    let account_id_cs = valid_account_id(session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    Ok((token_id_cs, account_id_cs))
}

pub struct XVerificationService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl XVerificationService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    /// Start OAuth: make PKCE, stash state→(account_id,verifier), return authorize URL.
    pub async fn start_login(&self, account_id: &str) -> Result<OAuthLoginResponse, AppError> {
        let (verifier, challenge) = client::generate_pkce();
        let state = {
            let bytes: [u8; 24] = rand::random();
            hex::encode(bytes)
        };
        let payload = XOAuthState {
            account_id: account_id.to_string(),
            code_verifier: verifier,
        };
        self.redis
            .set_x_oauth_state(&state, &payload, *crate::config::X_OAUTH_STATE_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save state: {e}")))?;
        Ok(OAuthLoginResponse {
            authorize_url: client::build_authorize_url(&state, &challenge),
        })
    }

    /// Log out: drop the pending X-OAuth login for this session (Redis
    /// `x_pending`). Idempotent — deleting a non-existent key is a no-op, so
    /// callers get `ok` regardless of prior login state.
    pub async fn logout(&self, account_id: &str) -> Result<(), AppError> {
        self.redis
            .delete_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("clear pending: {e}")))
    }

    /// Handle the OAuth callback: consume state, exchange code, fetch followers,
    /// stash pending. Returns the account_id the pending was stored under.
    pub async fn complete_callback(&self, code: &str, state: &str) -> Result<String, AppError> {
        let st = self
            .redis
            .get_and_delete_x_oauth_state(state)
            .await
            .map_err(|_| AppError::Gone("state_expired".into()))?;
        let token = client::exchange_code(code, &st.code_verifier)
            .await
            .map_err(|e| AppError::InternalError(format!("code exchange: {e}")))?;
        let me = client::get_me(&token.access_token)
            .await
            .map_err(|e| AppError::InternalError(format!("get_me: {e}")))?;
        let pending = XPending {
            x_user_id: me.id,
            access_token: token.access_token,
            followers_count: me.followers_count,
            followed_by: vec![],
        };
        self.redis
            .set_x_pending(&st.account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(st.account_id)
    }

    pub async fn add_followed_by(
        &self,
        account_id: &str,
        handle: &str,
    ) -> Result<FollowedByResponse, AppError> {
        let mut pending = self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;

        // Cap BEFORE spending an X API call.
        if pending.followed_by.len() >= *X_FOLLOWED_BY_MAX {
            return Err(AppError::BadRequest("max_followed_by_reached".into()));
        }

        let check = client::check_follows_me(&pending.access_token, handle)
            .await
            .map_err(|e| AppError::InternalError(format!("follow check: {e}")))?;

        if !check.is_following {
            return Ok(FollowedByResponse {
                is_following: false,
                entry: None,
            });
        }

        let entry = XFollowedByEntry {
            x_handle: check.x_handle,
            x_image_uri: check.x_image_uri,
            x_followers_count: check.x_followers_count,
            is_x_verified: check.is_x_verified,
        };
        append_followed_by(&mut pending.followed_by, entry.clone(), *X_FOLLOWED_BY_MAX)
            .map_err(|m| AppError::BadRequest(m.to_string()))?;
        self.redis
            .set_x_pending(account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(FollowedByResponse {
            is_following: true,
            entry: Some(entry),
        })
    }

    pub async fn remove_followed_by(
        &self,
        account_id: &str,
        handle: &str,
    ) -> Result<StatusResponse, AppError> {
        let mut pending = self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;
        pending
            .followed_by
            .retain(|e| !e.x_handle.eq_ignore_ascii_case(handle));
        self.redis
            .set_x_pending(account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(StatusResponse {
            followers_count: Some(pending.followers_count),
            followed_by: pending.followed_by,
        })
    }

    /// Public verification-progress view — omits the creator's own handle / user-id.
    pub async fn get_status(&self, account_id: &str) -> Result<StatusResponse, AppError> {
        match self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
        {
            Some(p) => Ok(StatusResponse {
                followers_count: Some(p.followers_count),
                followed_by: p.followed_by,
            }),
            None => Ok(StatusResponse {
                followers_count: None,
                followed_by: vec![],
            }),
        }
    }

    /// Pre-deploy reservation (design §3-bis, §7.1-B). Runs the (relocated)
    /// creator + CREATE2 checks, confirms the token is NOT yet deployed on-chain
    /// (fail-closed), then records an atomic first-writer-wins reservation.
    pub async fn reserve(
        &self,
        req: &ReserveRequest,
        session_address: &str,
    ) -> Result<(), AppError> {
        // 1. creator == session wallet (cheap first filter).
        creator_matches(&req.creator, session_address)?;
        // 2. CREATE2(version, salt) == token_id (salt-possession proof).
        let salt =
            B256::from_str(&req.salt).map_err(|_| AppError::BadRequest("invalid salt".into()))?;
        let expected = SaltService::compute_token_address(req.version.clone(), salt)?;
        address_matches(expected, &req.token_id)?;
        // 3. canonical checksummed ids (never LOWER()).
        let (token_id_cs, account_id_cs) = canonical_persist_ids(expected, session_address)?;
        // 4. THE boundary: token must NOT already be deployed (salt still secret).
        //    Fail-closed — an RPC error propagates as 503, never "assumed undeployed".
        if onchain::is_contract_deployed(&token_id_cs).await? {
            return Err(AppError::Conflict("already_deployed".into()));
        }
        // 5. first-writer-wins reservation.
        let controller = XVerificationController::new(self.postgres.clone());
        let owner = controller
            .reserve_first_writer(&token_id_cs, &account_id_cs)
            .await
            .map_err(|e| {
                error!("reserve persist failed: {e}");
                AppError::InternalError("reserve failed".into())
            })?;
        if owner != account_id_cs {
            return Err(AppError::Conflict("token_already_reserved".into()));
        }
        Ok(())
    }

    /// Reservation-only finalize (design §7.1-B). The CREATE2/creator checks now
    /// live in `reserve`; here we only prove the token_id was reserved by THIS
    /// session, then persist the pending signals. The reservation row is kept
    /// (not deleted) so re-finalize stays idempotent and only the reserver may
    /// ever refresh this token's verification.
    pub async fn finalize(
        &self,
        req: &FinalizeRequest,
        session_address: &str,
    ) -> Result<(), AppError> {
        // Canonicalize the client token_id + session to checksummed forms.
        let expected = Address::from_str(&req.token_id)
            .map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
        let (token_id_cs, account_id_cs) = canonical_persist_ids(expected, session_address)?;

        // 1. reservation must exist AND belong to this session.
        let controller = XVerificationController::new(self.postgres.clone());
        let owner = controller
            .reservation_owner(&token_id_cs)
            .await
            .map_err(|e| {
                error!("reservation lookup failed: {e}");
                AppError::InternalError("finalize failed".into())
            })?
            .ok_or_else(|| AppError::Forbidden("not_reserved".into()))?;
        if owner != account_id_cs {
            return Err(AppError::Forbidden("not_reserved".into()));
        }

        // 2. pending must still exist.
        let pending = self
            .redis
            .get_x_pending(session_address)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;

        // 3. persist (idempotent upsert) — canonical checksummed ids only.
        controller
            .finalize(
                &token_id_cs,
                &account_id_cs,
                &pending.x_user_id,
                pending.followers_count,
                &pending.followed_by,
            )
            .await
            .map_err(|e| {
                error!("finalize persist failed: {e}");
                AppError::InternalError("finalize failed".into())
            })?;

        // 4. clear pending (reservation row intentionally kept).
        let _ = self.redis.delete_x_pending(session_address).await;
        Ok(())
    }

    /// Public read for `/trade/xinfo/:token_id` — no auth, no ownership
    /// checks (this is intentionally public info once a coin is verified).
    pub async fn get_xinfo(
        &self,
        token_id: &str,
    ) -> Result<Option<crate::types::token::x_verification::TokenXVerification>, AppError> {
        let controller = XVerificationController::new(self.postgres.clone());
        controller.get_verification(token_id).await.map_err(|e| {
            error!("get_xinfo failed: {e}");
            AppError::InternalError("get_xinfo failed".into())
        })
    }
}

#[cfg(test)]
mod finalize_checks {
    // NOTE (Task 12): these pure helpers are now called by `reserve` (not
    // `finalize`); finalize is reservation-only. The checks are unchanged.
    use super::{address_matches, canonical_persist_ids, creator_matches};
    use crate::result::AppError;
    use crate::services::token::salt::SaltService;
    use alloy::primitives::{Address, B256};
    use std::str::FromStr;

    // Known-good deployer/impl (same fixtures as salt.rs tests). We derive a
    // REAL address from a salt with compute_create2_address so the test is
    // independent of env-loaded config.
    fn fixture_address() -> Address {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation =
            Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();
        SaltService::compute_create2_address(deployer, implementation, B256::from([7u8; 32]))
    }

    #[test]
    fn address_match_accepts_exact_and_rejects_other() {
        let addr = fixture_address();
        // exact (any case) passes
        assert!(address_matches(addr, &addr.to_string()).is_ok());
        // a different token_id is rejected with 403 creator_mismatch (IDOR guard)
        let other = "0x000000000000000000000000000000000000dEaD";
        let err = address_matches(addr, other).unwrap_err();
        assert!(matches!(err, AppError::Forbidden(ref m) if m == "creator_mismatch"));
    }

    #[test]
    fn creator_match_is_checksum_case_insensitive() {
        let a = "0x742d35Cc6634C0532925a3b844Bc9e7595f70143";
        // same address, different casing → OK (checksum compare, never LOWER())
        assert!(creator_matches(a, &a.to_lowercase()).is_ok());
        // different address → 403
        let b = "0x0000000000000000000000000000000000000001";
        let err = creator_matches(a, b).unwrap_err();
        assert!(matches!(err, AppError::Forbidden(ref m) if m == "creator_mismatch"));
    }

    // Regression test for the "persist non-canonical casing" finding: both
    // `address_matches` (byte-equality) and `creator_matches` (checksum
    // compare) accept a lowercase/mixed-case client value, but the values
    // handed to `XVerificationController::finalize` (via
    // `canonical_persist_ids`, called from `XVerificationService::finalize`)
    // must always be the EIP-55 checksum form — never the raw client/caller
    // casing — so Postgres joins/upserts against `token.token_id` and the
    // `token_x_verification(token_id)` unique constraint stay case-consistent.
    #[test]
    fn finalize_persists_checksummed_ids_not_raw_client_casing() {
        let addr = fixture_address();
        let lower_token_id = addr.to_string().to_lowercase();
        // sanity: the byte-equality check accepts the lowercase form (this is
        // the exact input that would previously have been persisted as-is).
        assert!(address_matches(addr, &lower_token_id).is_ok());

        let session_addr = Address::from_str("0x000000000000000000000000000000000000Aa01").unwrap();
        let checksum_session = session_addr.to_checksum(None);
        let lower_session = checksum_session.to_lowercase();
        assert!(creator_matches(&checksum_session, &lower_session).is_ok());

        let (token_id_cs, account_id_cs) =
            canonical_persist_ids(addr, &lower_session).expect("valid session address");

        assert_eq!(token_id_cs, addr.to_checksum(None));
        assert_ne!(
            token_id_cs, lower_token_id,
            "must persist the canonical checksum, not the client's lowercase token_id"
        );

        assert_eq!(account_id_cs, checksum_session);
        assert_ne!(
            account_id_cs, lower_session,
            "must persist the canonical checksum, not the caller's lowercase session address"
        );
    }

    #[test]
    fn canonical_persist_ids_rejects_invalid_session_address() {
        let addr = fixture_address();
        let err = canonical_persist_ids(addr, "not-an-address").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(ref m) if m == "invalid session address"));
    }
}
