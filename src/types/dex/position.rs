use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for `GET /dex/positions/:account_id`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionsResponse {
    pub account_id: String,
    pub positions: Vec<LpPositionEntry>,
}

/// One open LP position (single `(account_id, pool_id)` pair with `balance > 0`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionEntry {
    pub pool_id: String,
    /// `"CHOG-WMON"` — composed from `token0.symbol` + `"-"` + `token1.symbol`.
    pub pair_label: String,
    pub token0: LpPositionTokenSide,
    pub token1: LpPositionTokenSide,
    /// Raw LP balance (wei). Matches `lp_position.balance`.
    pub balance: String,
    /// `balance × pool.value / pool.total_supply`. NULL if `pool.total_supply = 0`.
    pub my_liquidity_usd: Option<String>,
    /// `pool.value` — current TVL snapshot in USD as maintained by the indexer.
    pub tvl_usd: String,
    /// 7d LP-net APR as a percentage (e.g. `130.0` = 130%). NULL when undefined
    /// (no `pool_apr` row for this pool, or `tvl_7d_usd_avg = 0`).
    pub apr_pct_7d: Option<String>,
}

/// Per-side token info + cost-basis deposited amount.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionTokenSide {
    pub token_id: String,
    pub symbol: String,
    pub decimals: i32,
    pub image_uri: String,
    /// `token0_in - token0_out` (or `token1_in - token1_out`). Raw wei string.
    /// Frozen at deposit time (cost basis), NOT live mark-to-market.
    pub deposited: String,
    /// USD value of `deposited`, frozen at deposit time. Sourced from
    /// `lp_position.token{0,1}_in_usd - token{0,1}_out_usd`.
    pub deposited_usd: String,
}
