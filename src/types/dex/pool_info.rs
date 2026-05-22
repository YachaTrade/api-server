use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Pool-level info common to /dex/pools/:pool_id and the nested entries
/// in /dex/positions/:account_id. Captures everything about the pool
/// itself (reserves, TVL, APR) — independent of which wallet is viewing.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolInfo {
    /// Pair contract address (EIP-55 checksum). Matches `pool.pool_id`.
    pub pool_id: String,
    /// Human-readable pair label, e.g. `"CHOG-WMON"`. Composed from
    /// `token0.symbol` + `"-"` + `token1.symbol`.
    pub pair_label: String,
    /// Raw `pool.reserve0` (wei).
    pub reserve0: String,
    /// Raw `pool.reserve1` (wei).
    pub reserve1: String,
    /// Pool TVL snapshot in USD — `pool.value`, maintained by the
    /// indexer alongside reserves when prices are known.
    pub tvl_usd: String,
    /// Total LP supply (wei) — `pool.total_supply`, maintained by the
    /// apply_lp_position() trigger on mint/burn events.
    pub total_supply: String,
    /// Maximum LP-net APR across the 24h/7d/30d windows from the
    /// `pool_apr` view, formatted as a percent string with 4 decimal
    /// places (e.g. `"130.0000"`). NULL when no pool_apr row exists
    /// for this pool OR all three windows lack data.
    pub apr: Option<String>,
}
