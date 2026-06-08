use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::dex::pool_info::PoolInfo;

/// Response for `GET /dex/pools/:pool_id`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolDetailResponse {
    /// Pool-level info (reserves, TVL, APR, etc.) shared with `/dex/positions`.
    pub pool_info: PoolInfo,
    /// Token at index 0 of the pool.
    pub token0: PoolTokenSide,
    /// Token at index 1 of the pool.
    pub token1: PoolTokenSide,
    /// Per-pair fee rates from `fee_config`. NULL when no fee_config
    /// row exists for this pool.
    pub fee_config: Option<FeeConfigInfo>,
}

/// One side of a pool. Display metadata (symbol/decimals/image) is sourced
/// from curated `whitelist_token` first, then `token`, `dex_token`,
/// `quote_token` — so a whitelisted pair (e.g. USDT/WMON) renders its
/// curated symbol and icon instead of a registry's empty/placeholder value.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolTokenSide {
    /// Token contract address (EIP-55 checksum).
    pub token_id: String,
    /// Token symbol. Whitelist-first, then `token`/`dex_token`/`quote_token`.
    /// Empty string when no registry has the token.
    pub symbol: String,
    /// Token decimals. Whitelist-first, then `dex_token`/`quote_token`.
    /// Defaults to 18 when missing from registries.
    pub decimals: i32,
    /// Token icon URL. Curated `whitelist_token` image wins, then
    /// `token`/`dex_token`/`quote_token`. Empty string when none provide one.
    pub image_uri: String,
}

/// Fee rates in basis points (1 bps = 0.01%). Sourced from `fee_config`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeeConfigInfo {
    pub creator_bps: i16,
    pub curve_protocol_bps: i16,
    pub dex_protocol_bps: i16,
}
