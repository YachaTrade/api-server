use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query params for `GET /dex/tokens`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DexTokenListQuery {
    /// Optional EIP-55 wallet address. When provided, each entry includes the
    /// user's balance for that token. Absent → balance omitted.
    pub account: Option<String>,
    /// Optional case-insensitive search across `symbol`, `name`, and `token_id`.
    /// When omitted, returns the full sorted list (paginated).
    pub q: Option<String>,
    /// Page size. Default 50, hard-capped at 200.
    pub limit: Option<i64>,
    /// Page offset (number of rows to skip). Default 0.
    pub offset: Option<i64>,
}

/// Response for `GET /dex/tokens`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DexTokenListResponse {
    pub tokens: Vec<DexTokenEntry>,
    /// Offset to pass on the next page request, or NULL when this page is the
    /// last one.
    pub next_offset: Option<i64>,
}

/// One token row in the Select Coin list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DexTokenEntry {
    pub token_id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: i32,
    pub image_uri: String,
    /// Raw wei balance for the queried account. Only present when `?account=` was
    /// supplied. `"0"` when the (account, token) pair has no balance row OR is
    /// zero, so FE can distinguish "queried but no balance" from "balance not asked for".
    pub balance: Option<String>,
    /// Token market cap in USD = `market.price × token.total_supply × quote_price_in_usd`.
    /// NULL when total_supply unknown (pure dex_token, not in `token` table) OR
    /// no `market` row OR no `price` row.
    pub market_cap_usd: Option<String>,
}
