use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::types::common::info::AccountInfo;

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct OrderTokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub market_cap: String,
    pub reserve_token: String,
    pub created_at: i64,
    pub market_type: String,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderToken {
    pub token_info: OrderTokenInfo,
    pub account_info: AccountInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderMessage {
    pub order_type: TokenOrderType,
    pub order_token: Option<Vec<OrderToken>>,
    pub king_of_the_hill: Option<OrderToken>,
    pub total_count: i64,
}
