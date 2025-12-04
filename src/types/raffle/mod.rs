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

/// Individual prize information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RafflePrize {
    /// Raffle entry ID that won
    pub raffle_id: i32,
    /// Prize rank (1st, 2nd, 3rd, etc.)
    pub rank: i32,
    /// Prize type: GENERAL_MONAD, GENERAL_HYPE, MONAD_AIRDROP_MONAD, MONAD_AIRDROP_HYPE
    pub prize_type: String,
    /// Transaction hash if prize has been sent
    pub transaction_hash: Option<String>,
    /// Prize amount
    pub amount: String,
}

/// Raffle check response with entries and prizes
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleCheckResponse {
    /// Round number
    pub round: i64,
    /// User's account address
    pub account_id: String,
    /// Total raffle entries for this round
    pub total_raffle: i64,
    /// Number of winning entries
    pub total_wins: i64,
    /// Total prize amount won
    pub total_amount: String,
    /// List of prizes won
    pub prizes: Vec<RafflePrize>,
}
