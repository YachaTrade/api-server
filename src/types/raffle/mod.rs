use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RaffleStatusResponse {
    pub is_eligible: bool,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Prize {
    pub account_id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrizeListResponse {
    pub prizes: Vec<Prize>,
}
