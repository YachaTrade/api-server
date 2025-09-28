use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::types::common::info::{
    AccountInfo, MarketInfo, PositionInfo, PositionTokenInfo, TokenInfo,
};

/// Position information for a token held by a profile
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Position {
    /// Token information
    pub token: PositionTokenInfo,

    /// Unique position identifier
    pub position: PositionInfo,

    pub market: MarketInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionResponse {
    pub positions: Vec<Position>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolder {
    pub current_amount: String,
    pub account_info: AccountInfo,
    pub is_dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolderResponse {
    pub holders: Vec<TokenHolder>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldToken {
    pub token_info: TokenInfo,
    pub balance: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldTokenResponse {
    pub tokens: Vec<HoldToken>,
    pub total_count: i64,
}
