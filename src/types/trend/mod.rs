use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{MarketInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrendToken {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    pub percent: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrendResponse {
    pub tokens: Vec<TrendToken>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrendRequest {
    pub token_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrendActionResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminRequest {
    pub account_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminActionResponse {
    pub success: bool,
}
