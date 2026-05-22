use crate::types::common::pagination::{default_page, deserialize_limit, deserialize_page};
use serde::Deserialize;
use utoipa::IntoParams;

fn default_dex_search_limit() -> i64 {
    50
}

/// Query params for `GET /dex/search`. Requires a valid session cookie —
/// the user's account address is derived from the session, not query params.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DexSearchQuery {
    /// Required case-insensitive search term. Matched via ILIKE against
    /// `symbol`, `name`, and `token_id` of pool-backed tokens.
    pub q: String,
    /// 1-indexed page number. Default 1.
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    /// Page size. Default 50, hard-capped at 100.
    #[serde(default = "default_dex_search_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
}
