use crate::types::common::pagination::{default_page, deserialize_limit, deserialize_page};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

fn default_dex_tokens_limit() -> i64 {
    50
}

/// Query params for `GET /dex/tokens`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct DexTokenListQuery {
    /// Optional EIP-55 wallet address. When provided, each entry includes the
    /// user's balance for that token. Absent → balance omitted.
    pub account: Option<String>,
    /// Optional case-insensitive search term. Absent → 4-tier default list.
    /// Present → prefix search (symbol/name/CA), external은 full CA exact 매칭만.
    pub q: Option<String>,
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

/// One token row in the Select Token modal list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DexTokenEntry {
    pub token_id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: i32,
    pub image_uri: String,
    /// "whitelist" | "nadfun_v2" | "external". FE 렌더 분기용 (external 판정 = token_type == "external").
    pub token_type: String,
    /// 보유(balance > 0) 시에만 raw wei balance. 미보유·account 미제공이면 null → FE는 balance != null로 보유 판정.
    pub balance: Option<String>,
    /// 보유분 USD 가치 = balance/10^decimals × market.price × price. 미보유면 None.
    pub balance_usd: Option<String>,
}
