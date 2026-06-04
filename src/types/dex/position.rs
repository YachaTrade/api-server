use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::dex::pool_info::PoolInfo;

/// Response for `GET /dex/positions/:account_id` — every open LP position the
/// wallet holds across V2 pools.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionsResponse {
    /// Wallet address (EIP-55 checksum). Echoes the path param.
    pub account_id: String,
    /// One entry per open `(account, pool)` pair (i.e. `balance > 0`).
    /// Ordered by `pool_id` ascending for deterministic pagination/UI.
    pub positions: Vec<LpPositionEntry>,
}

/// A single open LP position. "Open" means `lp_position.balance > 0` —
/// wallets that fully exited a pool are omitted.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionEntry {
    /// Pool-level info (reserves, TVL, APR, total supply, etc.) — same
    /// shape as `/dex/pools/:pool_id` response's `pool` field.
    pub pool_info: PoolInfo,
    /// Token at index 0 of the pool. Includes metadata, cost basis
    /// (deposited), and current pro-rata share.
    pub token0: LpPositionTokenSide,
    /// Token at index 1 of the pool. Same shape as `token0`.
    pub token1: LpPositionTokenSide,
    /// LP token balance held by the wallet (raw wei). Matches the
    /// stored-generated column `lp_position.balance = lp_in - lp_out`.
    pub balance: String,
    /// CURRENT mark-to-market liquidity in USD =
    /// `balance × pool.tvl_usd / pool.total_supply`, truncated to 8 decimals.
    /// NULL when `pool.total_supply = 0`.
    pub liquidity_usd: Option<String>,
}

/// One side of an LP position — token metadata plus the wallet's
/// per-side cost basis (deposited) and current pro-rata holdings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionTokenSide {
    /// Token contract address (EIP-55 checksum).
    pub token_id: String,
    /// Token symbol — sourced from `token`, falling back to
    /// `dex_token`, then `quote_token`. Empty string when none of
    /// the three registries has the token (shouldn't happen for
    /// pool-backed tokens).
    pub symbol: String,
    /// Token decimals. Defaults to 18 when missing from registries.
    pub decimals: i32,
    /// Token icon URL. Empty string when none of the registries
    /// provide one.
    pub image_uri: String,
    /// Cost-basis amount of this token (raw wei) deposited by the
    /// wallet across all mints, net of burns at this side's reserve
    /// share. FROZEN at deposit time, NOT live mark-to-market.
    /// Sourced from `lp_position.token{0,1}_in - token{0,1}_out`.
    pub deposit_amount: String,
    /// USD value of `deposit_amount` at the time the deposit/withdraw
    /// happened (block-time price). FROZEN — does NOT track current
    /// price. Sourced from
    /// `lp_position.token{0,1}_in_usd - token{0,1}_out_usd`.
    pub deposit_usd: String,
    /// CURRENT pro-rata amount of this token claimable on a full
    /// withdraw NOW (**raw wei, integer**) =
    /// `floor(balance × pool.reserve{0,1} / pool.total_supply)`.
    /// Floored to a whole number — there is no fractional wei.
    /// Live mark-to-market — reflects the latest reserves indexed.
    /// NULL when `pool.total_supply = 0`.
    pub current_amount: Option<String>,
}
