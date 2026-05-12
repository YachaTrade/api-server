use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Vault classification — mirrors `v2_vault_metadata.vault_type` CHECK constraint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "VARCHAR")]
pub enum VaultType {
    #[serde(rename = "BURN")]
    #[sqlx(rename = "BURN")]
    Burn,
    #[serde(rename = "LP")]
    #[sqlx(rename = "LP")]
    Lp,
    #[serde(rename = "CREATOR_FEE")]
    #[sqlx(rename = "CREATOR_FEE")]
    CreatorFee,
    #[serde(rename = "GIFT")]
    #[sqlx(rename = "GIFT")]
    Gift,
    #[serde(rename = "CUSTOM")]
    #[sqlx(rename = "CUSTOM")]
    Custom,
}

/// Top-level response for `GET /vault/{token_id}`.
///
/// Each entry represents a vault that the token routes a portion of its
/// trading fees to. The list is derived from `v2_creator_fee_allocation`
/// (membership + bps) joined with `v2_vault_metadata` (type + name).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenVaultsResponse {
    pub token_id: String,
    /// Quote token denomination for `total_quote_amount` and each vault's
    /// `quote_amount`. Sourced from `market.quote_id`.
    pub quote_id: String,
    /// Sum of `quote_amount` across all vaults for this token.
    /// Equal to `SUM(v2_creator_fee_distribution_stats.distributed_quote)`
    /// over rows where `token_id = $1`.
    pub total_quote_amount: String,
    /// USD-denominated sum, evaluated at fee-distribution time (not at
    /// request time). Equal to
    /// `SUM(v2_creator_fee_distribution_stats.distributed_quote_usd)`.
    pub total_quote_amount_usd: String,
    pub vaults: Vec<VaultEntry>,
}

/// Common fields shared by every vault, regardless of type.
///
/// Type-specific stats are nested under `stats` via the `VaultStats` enum.
/// `last_executed_at` is hoisted because every vault stat table tracks it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VaultEntry {
    pub vault_id: String,
    pub bps: u32,
    pub name: String,
    pub active: bool,
    /// Cumulative quote-denominated fee distributed to this vault for the
    /// token. Sourced from `v2_creator_fee_distribution_stats.distributed_quote`.
    /// Denomination given by the parent response's `quote_id`.
    pub quote_amount: String,
    /// USD-denominated cumulative fee distributed to this vault, evaluated at
    /// each distribution time. Sourced from
    /// `v2_creator_fee_distribution_stats.distributed_quote_usd`.
    pub quote_amount_usd: String,
    /// Latest stat-table `updated_at` for this vault (unix seconds).
    /// `0` when the vault has no recorded activity yet.
    pub last_executed_at: i64,
    #[serde(flatten)]
    pub stats: VaultStats,
}

/// Tagged union over per-vault-type stats. Discriminant is `vault_type`
/// at the same JSON level as `VaultEntry`'s common fields (via `flatten`),
/// payload lives under `stats`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "vault_type", content = "stats")]
pub enum VaultStats {
    #[serde(rename = "BURN")]
    Burn(BurnStats),
    #[serde(rename = "LP")]
    Lp(LpStats),
    #[serde(rename = "CREATOR_FEE")]
    CreatorFee(CreatorFeeStats),
    #[serde(rename = "GIFT")]
    Gift(GiftStats),
    /// Catch-all for vaults whose type isn't one of the four standard ones.
    /// Stats payload is intentionally empty — the caller can still render
    /// `vault_id` / `bps` / `name` from `VaultEntry`.
    #[serde(rename = "CUSTOM")]
    Custom(EmptyStats),
}

/// Buyback & Burn vault — `v2_burn_vault_stats`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BurnStats {
    /// MON spent on buyback (`quote_spent`).
    pub quote_spent: String,
    /// USD value of `quote_spent`, evaluated at execution time.
    pub quote_spent_usd: String,
    /// Tokens permanently burned (`tokens_burned`).
    pub tokens_burned: String,
    /// Number of buyback+burn executions (`burn_count`).
    pub execution_count: i32,
}

/// LP Support vault — `v2_lp_vault_stats`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpStats {
    /// MON injected into the LP pool (`quote_injected`).
    pub quote_injected: String,
    /// USD value of `quote_injected`, evaluated at execution time.
    pub quote_injected_usd: String,
    /// Tokens injected alongside the quote (`token_injected`).
    pub token_injected: String,
    /// LP tokens locked/burned (`lp_burned`).
    pub lp_burned: String,
    /// Pool pair label (e.g. `"ATOM/WMON"`). Composed from the token
    /// symbol and the quote asset symbol — not stored on chain.
    pub pool_pair: String,
    /// Number of LP injections (`inject_count`).
    pub execution_count: i32,
}

/// Creator share vault — `v2_creator_fee_vault_stats`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatorFeeStats {
    /// Current creator wallet bound to the token (EIP-55 checksum).
    /// Sourced from `token.creator`, which V2's trigger keeps in sync with
    /// the latest `v2_creator_updates.new_creator` (SETUP or UPDATE event).
    #[serde(default)]
    pub creator_id: String,
    /// Unclaimed balance currently sitting in the vault.
    pub current_balance: String,
    /// USD value of `current_balance`, evaluated at request time using the
    /// latest `price.price` for the token's `quote_id`.
    pub current_balance_usd: String,
    /// Lifetime MON deposited into the vault.
    pub total_deposited: String,
    /// USD value of `total_deposited`, evaluated at each deposit time.
    pub total_deposited_usd: String,
    /// Lifetime MON claimed by the creator.
    pub total_claimed: String,
    /// USD value of `total_claimed`, evaluated at each claim time.
    pub total_claimed_usd: String,
    pub deposit_count: i32,
    pub claim_count: i32,
}

/// Gift vault — `v2_gift_vault_stats`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GiftStats {
    /// Lifecycle state: `"Accumulating" | "Active" | "Burned"`.
    pub current_state: String,
    pub current_balance: String,
    /// USD value of `current_balance`, evaluated at request time using the
    /// latest `price.price` for the token's `quote_id`.
    pub current_balance_usd: String,
    pub total_deposited: String,
    /// USD value of `total_deposited`, evaluated at each deposit time.
    pub total_deposited_usd: String,
    /// MON sent to the receiver across all CLAIM events.
    pub total_claimed: String,
    /// USD value of `total_claimed`, evaluated at each claim time.
    pub total_claimed_usd: String,
    /// MON burned via expiry sweep (terminal state).
    pub total_expired: String,
    /// USD value of `total_expired`, evaluated at expiry time.
    pub total_expired_usd: String,
    /// Recipient platform: `"X"` or `"GITHUB"`.
    pub platform: Option<String>,
    /// Recipient handle on the platform (e.g. `"@Beakdoong"`).
    pub platform_id: Option<String>,
    /// On-chain wallet address bound to the gift, if `RECEIVER_SET` fired.
    pub receiver: Option<String>,
    /// MON spent on buyback when the gift was burned.
    pub buyback_quote_spent: String,
    /// USD value of `buyback_quote_spent`, evaluated at buyback time.
    pub buyback_quote_spent_usd: String,
    /// Tokens burned during the buyback sweep.
    pub buyback_tokens: String,
    /// Verification deadline for an unbound gift (unix seconds). Equals the
    /// SETUP event's `block_timestamp + GIFT_EXPIRY_DURATION`. Cleared to
    /// `0` once RECEIVER_SET fires (gift bound, no longer expires). Stays
    /// `0` for tokens that never had a SETUP event. Sourced from
    /// `v2_gift_vault_stats.expires_at`.
    #[serde(default)]
    pub expires_at: i64,
    /// Block timestamp of the RECEIVER_SET event that bound this gift
    /// (unix seconds). `None` until verification fires. Sourced from
    /// `v2_gift_vault_stats.receiver_set_at` (DB stores `0` as the
    /// not-yet-set sentinel; mapped to `None` here).
    #[serde(default)]
    pub receiver_set_at: Option<i64>,
}

/// Empty stats payload for `CUSTOM` vaults — intentionally a struct (not a
/// unit variant) so the JSON shape stays consistent (`"stats": {}`) and
/// `utoipa` generates a schema for it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmptyStats {}
