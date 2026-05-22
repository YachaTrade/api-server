use crate::types::common::pagination::{default_page, deserialize_limit, deserialize_page};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

fn default_dex_tokens_limit() -> i64 {
    50
}

/// Query params for `GET /dex/tokens`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DexTokenListQuery {
    /// Optional EIP-55 wallet address. When provided, each entry includes the
    /// user's balance for that token. Absent → balance omitted.
    pub account: Option<String>,
    /// 1-indexed page number. Default 1.
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    /// Page size. Default 50, hard-capped at 100.
    #[serde(default = "default_dex_tokens_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
}

/// Response for `GET /dex/tokens`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DexTokenListResponse {
    pub tokens: Vec<DexTokenEntry>,
    /// Total number of tokens matching the query (before pagination).
    pub total_count: i64,
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
