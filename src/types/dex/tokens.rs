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
    /// "whitelist" | "nadfun_v2" | "external". FE 렌더 분기용.
    pub token_type: String,
    /// nad.fun/화이트리스트가 아닌 외부 토큰. true면 grey 첫글자 아이콘 + 경고 + CA 표시.
    pub is_external: bool,
    /// balance > 0 여부. account 미제공 시 항상 false.
    pub is_held: bool,
    /// Raw wei balance. `?account=` 제공 시에만 Some.
    pub balance: Option<String>,
    /// 보유분 USD 가치 = balance/10^decimals × market.price × price. 가격 없으면 None.
    pub balance_usd: Option<String>,
    /// 마켓캡 USD. nadfun(total_supply 보유)만 계산, external은 None.
    pub market_cap_usd: Option<String>,
    /// 1=보유 화이트리스트, 2=보유 V2, 3=미보유 화이트리스트, 4=미보유 V2. FE 섹션 헤더용.
    pub tier: i32,
}
