use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfoWithDescription {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    #[serde(default)]
    pub follower_count: i32,
    #[serde(default)]
    pub following_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountInfoWithX {
    pub account_info: AccountInfo,
    pub x_info: Option<XInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PositionTokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub created_at: i64,
    pub total_supply: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MarketInfo {
    pub market_id: String,
    pub market_type: String,
    pub virtual_token: BigDecimal,
    pub virtual_native: BigDecimal,
    pub reserve_token: BigDecimal,
    pub reserve_native: BigDecimal,
    pub price: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PositionInfo {
    pub total_bought_native: BigDecimal,
    pub total_bought_token: BigDecimal,
    pub current_token_amount: BigDecimal,
    pub current_value: BigDecimal,
    pub realized_pnl: BigDecimal,
    pub unrealized_pnl: BigDecimal,
    pub total_pnl: BigDecimal,
    pub created_at: i64,
    pub last_traded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct XInfo {
    pub x_handle: String,
    pub x_image_uri: String,
    pub is_blue_label: bool,
}
