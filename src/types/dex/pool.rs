use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::dex::pool_info::PoolInfo;

/// Response for `GET /dex/pools/:pool_id`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolDetailResponse {
    /// Pool-level info (reserves, TVL, APR, etc.) shared with `/dex/positions`.
    pub pool: PoolInfo,
    /// Token at index 0 of the pool.
    pub token0: PoolTokenSide,
    /// Token at index 1 of the pool.
    pub token1: PoolTokenSide,
    /// Per-pair fee rates from `fee_config`. NULL when no fee_config
    /// row exists for this pool.
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
