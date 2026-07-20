use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::types::common::info::{AccountInfo, MarketInfo, QuoteInfo, RewardInfo, TokenInfo};
use crate::types::common::pagination::{default_page, deserialize_limit, deserialize_page};

fn default_dividend_tokens_limit() -> i64 {
    50
}

/// Query params for `GET /dividend/tokens` — candidate dividend-payout tokens.
/// Candidate set = whitelist(enabled) ∪ V1(graduated) ∪ V2(all).
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct DividendTokenQuery {
    /// Optional case-insensitive search term (symbol / name / address substring).
    /// Absent → full candidate list.
    pub q: Option<String>,
    /// 1-indexed page number. Default 1.
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    /// Page size. Default 50, hard-capped at 100.
    #[serde(
        default = "default_dividend_tokens_limit",
        deserialize_with = "deserialize_limit"
    )]
    pub limit: i64,
}

// ============================================================================
// ① Profile Dividend — GET /profile/dividend/:account_id
//    Mirrors CreatedTokensResponse / TokenCreatedInfo so the UI reuses the
//    same card layout.
// ============================================================================

/// A wallet's dividend-bearing tokens with per-dividend-token claim info.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendTokensResponse {
    pub tokens: Vec<DividendTokenInfo>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendTokenInfo {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    /// One entry per dividend token paid out for this source token.
    pub rewards: Vec<DividendReward>,
    /// Last time the holder claimed any dividend for this token (epoch secs).
    pub last_claimed_at: Option<i64>,
    /// Token-row claimed-USD total = Σ rewards[].claimed_usd (the "Claimed Dividend" column).
    pub claimed_usd: String,
    /// Token-row claimable-USD total = Σ rewards[].claimable_usd (the "Claimable Dividend" column).
    pub claimable_usd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendReward {
    pub dividend_token_info: QuoteInfo,
    /// `amount` = cumulative accrued merkle leaf, `claimed_amount` = cumulative
    /// claimed, `claimable` = (amount - claimed_amount) > 0, `proof` = merkle
    /// proof for the on-chain claim tx. Claimable amount = amount - claimed_amount.
    pub reward_info: RewardInfo,
    /// Cumulative claimed value in USD for this dividend token
    /// (Σ dividend_claims.usd_value; 0 when the token is unpriceable).
    pub claimed_usd: String,
    /// Claimable value in USD = max(amount - claimed_amount, 0)/10^decimals ×
    /// latest dividend-token USD price (price_usd → dex_token_price → price → 0).
    pub claimable_usd: String,
}

// ============================================================================
// ② Trade Dividend — GET /dividend/holders/:token_id
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendHoldersResponse {
    pub token_info: TokenInfo,
    /// Dividend-token breakdown header (e.g. XAUt0 25%, USDT 25%).
    pub dividend_tokens: Vec<DividendRatioInfo>,
    pub holders: Vec<DividendHolderInfo>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendRatioInfo {
    pub dividend_token_info: QuoteInfo,
    /// Distribution ratio in BPS (2500 = 25%).
    pub ratio_bps: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DividendHolderInfo {
    pub holder: AccountInfo,
    /// Σ(accrued × USD price) across all dividend tokens (cumulative accrued).
    pub total_value_usd: String,
    pub last_received_at: i64,
}
