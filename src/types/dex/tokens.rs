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

/// `DexTokenEntry.token_type` — 토큰 분류(테이블 멤버십 기준). FE 렌더 분기용.
///
/// - `whitelist`: 큐레이션된 `whitelist_token`(enabled)
/// - `nadfun_v2` / `nadfun_v1`: `token.version` 이 V2 / V1
/// - `external`: 위 어디에도 없음 — indexed `dex_token` 단독 또는 RPC 온체인 메타 fallback
///
/// 기본 리스트(`q` 없음)는 `whitelist` + `nadfun_v2`만, `nadfun_v1`/`external`은 검색에서만 노출.
/// external 판정 = `token_type == "external"`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DexTokenType {
    Whitelist,
    NadfunV2,
    NadfunV1,
    External,
}

/// One token row in the Select Token modal list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DexTokenEntry {
    pub token_id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: i32,
    pub image_uri: String,
    /// 토큰 분류 — 가능한 값과 의미는 [`DexTokenType`] 참고.
    /// 런타임은 string이며 `whitelist` | `nadfun_v2` | `nadfun_v1` | `external` 중 하나.
    #[schema(value_type = DexTokenType, example = "whitelist")]
    pub token_type: String,
    /// 보유(balance > 0) 시에만 raw wei balance. 미보유·account 미제공이면 null → FE는 balance != null로 보유 판정.
    pub balance: Option<String>,
    /// 보유분 USD 가치 = balance/10^decimals × market.price × price. 미보유면 None.
    pub balance_usd: Option<String>,
    /// 토큰 1개당 USD 단가. **account 무관**(보유 여부와 상관없이 항상 제공).
    /// nadfun_v2/external = dex_token_price 뷰(deepest-TVL 풀의 per-token USD) 우선,
    /// 없으면 market.price × quote→USD fallback. whitelist = DefiLlama.
    /// 소수점 8자리까지 truncate. 가격 미상이면 None.
    pub price_usd: Option<String>,
}
