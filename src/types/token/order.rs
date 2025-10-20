use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{AccountInfo, MarketInfo, TokenInfo};

#[derive(Debug, Copy, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenOrderType {
    MarketCap,    // price * reserve_token
    CreationTime, // created_at
    LatestTrade,  // market latest_trade_at
    Verified,     // verified
}

impl TokenOrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenOrderType::MarketCap => "market_cap",
            TokenOrderType::CreationTime => "creation_time",
            TokenOrderType::LatestTrade => "latest_trade",
            TokenOrderType::Verified => "verified",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderToken {
    pub account_info: AccountInfo,
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderTokenResponse {
    pub tokens: Vec<OrderToken>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderMessage {
    pub order_type: TokenOrderType,
    pub order_token: Option<Vec<OrderToken>>,
    pub king_of_the_hill: Option<OrderToken>,
    pub total_count: i64,
}
