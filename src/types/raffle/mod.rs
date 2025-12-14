use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Current raffle round information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleRoundResponse {
    /// Round number
    pub round: i64,
    /// Round status (ACTIVE or COMPLETED)
    pub status: String,
    /// Round start timestamp
    pub start_at: i64,
    /// Round end timestamp
    pub end_at: i64,
}

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
    /// Total amount won from monad raffle
    pub total_monad: String,
    /// Total amount won from hype raffle
    pub total_hype: String,
}

/// Round epoch information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleRound {
    /// Round start timestamp
    pub start_at: i64,
    /// Round number
    pub round: i64,
    /// Round end timestamp
    pub end_at: i64,
}

/// Raffle check response with entries and prizes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleCheckResponse {
    /// Whether user has raffle entries in this round
    pub is_validate: bool,
    /// Round information
    pub round: RaffleRound,
    /// User's account address
    pub account_id: String,
    /// Prize amounts by type
    pub prizes: RafflePrizes,
}
