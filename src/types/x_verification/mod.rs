use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::TokenVersion;
use crate::types::token::x_verification::XFollowedByEntry;

// ---- Redis-serialized (never returned to clients directly) ----

/// `x_oauth:state:{state}` payload. Consumed once via GETDEL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XOAuthState {
    pub account_id: String,
    pub code_verifier: String,
}

/// `x_pending:{account_id}` payload. Holds the creator's short-lived token +
/// collected signals until finalize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XPending {
    pub x_user_id: String,
    pub access_token: String,
    pub followers_count: i64,
    #[serde(default)]
    pub followed_by: Vec<XFollowedByEntry>,
}

// ---- Request / response DTOs ----

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OAuthLoginResponse {
    pub authorize_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FollowedByRequest {
    pub handle: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FollowedByResponse {
    pub is_following: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<XFollowedByEntry>,
}

/// Public pending view — the creator's own handle / user-id are intentionally absent.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PendingResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers_count: Option<i64>,
    pub followed_by: Vec<XFollowedByEntry>,
}

/// Pre-deploy reservation request (design §3-bis). Sent right after
/// `/token/salt`, before on-chain deployment. `version` is REQUIRED (no serde
/// default) so a V2 token is never silently CREATE2-checked against V1
/// impl/deployer (resolves I1). `salt` proves salt-possession; `creator` must
/// equal the session wallet.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReserveRequest {
    pub token_id: String,
    pub creator: String,
    pub salt: String,
    pub version: TokenVersion,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReserveResponse {
    pub ok: bool,
}

/// Finalize is reservation-only (design §7.1-B): the CREATE2/creator/salt/
/// version checks moved to `reserve`. Only the token_id is needed; ownership is
/// proven by the pre-existing reservation.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinalizeRequest {
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FinalizeResponse {
    pub ok: bool,
}

// ---- Pure helpers ----

/// X handle rule: 1-15 chars of [A-Za-z0-9_].
pub fn validate_handle(handle: &str) -> bool {
    let len = handle.chars().count();
    (1..=15).contains(&len)
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Append with case-insensitive de-dup by handle; enforce `max`.
/// Err("max_followed_by_reached") when already at capacity with a new handle.
pub fn append_followed_by(
    list: &mut Vec<XFollowedByEntry>,
    entry: XFollowedByEntry,
    max: usize,
) -> Result<(), &'static str> {
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.x_handle.eq_ignore_ascii_case(&entry.x_handle))
    {
        *existing = entry; // refresh snapshot
        return Ok(());
    }
    if list.len() >= max {
        return Err("max_followed_by_reached");
    }
    list.push(entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(h: &str) -> XFollowedByEntry {
        XFollowedByEntry {
            x_handle: h.into(),
            x_image_uri: "u".into(),
            x_followers_count: 1,
            is_x_verified: false,
        }
    }

    #[test]
    fn handle_validation() {
        assert!(validate_handle("elonmusk"));
        assert!(validate_handle("a_1"));
        assert!(!validate_handle(""));
        assert!(!validate_handle("waytoolonghandle16"));
        assert!(!validate_handle("has space"));
        assert!(!validate_handle("bad-dash"));
    }

    #[test]
    fn append_enforces_max_and_dedup() {
        let mut list = vec![];
        assert!(append_followed_by(&mut list, e("a"), 3).is_ok());
        assert!(append_followed_by(&mut list, e("b"), 3).is_ok());
        // case-insensitive dedup replaces, does not grow
        assert!(append_followed_by(&mut list, e("A"), 3).is_ok());
        assert_eq!(list.len(), 2);
        assert!(append_followed_by(&mut list, e("c"), 3).is_ok());
        // now full → new handle rejected
        assert_eq!(
            append_followed_by(&mut list, e("d"), 3),
            Err("max_followed_by_reached")
        );
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn reserve_request_requires_version() {
        // Missing `version` must FAIL to deserialize (no silent V1 default → I1 fix).
        let missing = r#"{"token_id":"0xabc","creator":"0xdef","salt":"0x01"}"#;
        assert!(serde_json::from_str::<ReserveRequest>(missing).is_err());

        let ok = r#"{"token_id":"0xabc","creator":"0xdef","salt":"0x01","version":"V2"}"#;
        let req: ReserveRequest = serde_json::from_str(ok).unwrap();
        assert!(matches!(req.version, TokenVersion::V2));
    }

    #[test]
    fn finalize_request_is_token_id_only() {
        let req: FinalizeRequest = serde_json::from_str(r#"{"token_id":"0xabc"}"#).unwrap();
        assert_eq!(req.token_id, "0xabc");
    }
}
