use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for `GET /dex/pools/:pool_id`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolDetailResponse {
    pub pool_id: String,
    /// `"MON-CHOG"` — composed from `token0.symbol` + `"-"` + `token1.symbol`.
    pub pair_label: String,
    pub token0: PoolTokenSide,
    pub token1: PoolTokenSide,
    /// Raw `pool.reserve0` (wei) as a string to preserve precision.
    pub reserve0: String,
    /// Raw `pool.reserve1` (wei) as a string to preserve precision.
    pub reserve1: String,
    /// `pool.value` — current TVL snapshot in USD as maintained by the indexer.
    pub tvl_usd: String,
    /// `pool.total_supply` — current LP token supply (wei).
    pub total_supply: String,
    /// 7d LP-net APR as a percentage (e.g. `"130.0000"` = 130%). NULL when undefined
    /// (no `pool_apr` row for this pool, or `tvl_7d_usd_avg = 0`). Same formula
    /// as the `/dex/positions/:account_id` endpoint.
    pub apr: Option<String>,
    /// Per-pair fee rates from `fee_config`. NULL when no `fee_config` row exists.
    pub fee_config: Option<FeeConfigInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolTokenSide {
    pub token_id: String,
    pub symbol: String,
    pub decimals: i32,
    pub image_uri: String,
}

/// Fee rates in basis points (1 bps = 0.01%). Sourced from `fee_config`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeeConfigInfo {
    pub creator_bps: i16,
    pub curve_protocol_bps: i16,
    pub dex_protocol_bps: i16,
}
