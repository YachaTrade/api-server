use crate::types::common::pagination::{default_page, deserialize_limit, deserialize_page};
use serde::Deserialize;
use utoipa::IntoParams;

fn default_dex_search_limit() -> i64 {
    50
}

/// Query params for `GET /dex/search` (DEPRECATED — use `GET /dex/tokens?q=`).
/// Requires a valid session cookie; the account address is derived from the
/// session. Delegates to the unified `/dex/tokens` logic.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DexSearchQuery {
    /// Required search term, case-insensitive. Delegated to `/dex/tokens?q=`:
    /// prefix match on `symbol`/`name` (or `token_id` for a `0x` prefix); an
    /// external token surfaces only on an exact full contract address.
    pub q: String,
    /// 1-indexed page number. Default 1.
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    /// Page size. Default 50, hard-capped at 100.
    #[serde(default = "default_dex_search_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
}
