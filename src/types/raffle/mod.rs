use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Raffle eligibility status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleStatusResponse {
    /// Whether user is eligible for raffle
    pub is_eligible: bool,
    /// Number of raffle entries in current active round
    pub count: i64,
}

/// Query parameters for raffle check
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleCheckQuery {
    /// Round number to check
    pub round: i64,
}

/// Prize amounts by type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RafflePrizes {
    /// Total amount won from general monad raffle
    pub general_monad: String,
    /// Total amount won from general hype raffle
    pub general_hype: String,
    /// Total amount won from monad airdrop monad raffle
    pub monad_airdrop_monad: String,
    /// Total amount won from monad airdrop hype raffle
    pub monad_airdrop_hype: String,
}

/// Raffle check response with entries and prizes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleCheckResponse {
    /// Round number
    pub round: i64,
    /// User's account address
    pub account_id: String,
    /// Prize amounts by type
    pub prizes: RafflePrizes,
}
