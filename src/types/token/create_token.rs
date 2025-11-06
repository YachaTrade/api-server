use crate::types::common::info::TokenInfo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedResponse {
    pub tokens: Vec<TokenCreated>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreated {
    pub token: TokenInfo,
    pub is_graduated: bool,
    pub created_at: i64,
    pub market_cap: String,     //market cap -> price * total_supply
    pub total_supply: String,   // token table total_supply
    pub price: String,          // market table price
    pub current_amount: String, //position table current_token_amount
    pub description: Option<String>,
}
