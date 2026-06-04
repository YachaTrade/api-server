use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query params for `GET /dex/reserves`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ReservesQuery {
    /// Pool (pair) address (EIP-55 checksum).
    pub pool_id: String,
}

/// Current reserves of a V2 pool — a backend-served drop-in for an on-chain
/// `lpPair.getReserves()` call. `reserve0`/`reserve1` are raw wei, sourced from
/// the indexer-maintained `pool` row, so this endpoint makes **no RPC call**.
/// `reserve0`/`reserve1` correspond to `token0`/`token1` (same ordering as the
/// pair contract); the client orients in/out and applies the swap math.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReservesResponse {
    /// Pool (pair) address (EIP-55 checksum).
    pub pool_id: String,
    /// token0 address (EIP-55 checksum).
    pub token0: String,
    /// token1 address (EIP-55 checksum).
    pub token1: String,
    /// Raw `pool.reserve0` (wei).
    pub reserve0: String,
    /// Raw `pool.reserve1` (wei).
    pub reserve1: String,
    /// Block number of the indexer snapshot these reserves came from —
    /// lets the client gauge freshness vs the chain head.
    pub block_number: i64,
}
