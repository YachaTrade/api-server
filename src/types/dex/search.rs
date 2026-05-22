use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /dex/search`. Requires a valid session cookie —
/// the user's account address is derived from the session, not query params.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DexSearchQuery {
    /// Required case-insensitive search term. Matched via ILIKE against
    /// `symbol`, `name`, and `token_id` of pool-backed tokens.
    pub q: String,
    /// Page size. Default 50, capped at 200.
    pub limit: Option<i64>,
    /// Page offset (rows to skip). Default 0.
    pub offset: Option<i64>,
}
