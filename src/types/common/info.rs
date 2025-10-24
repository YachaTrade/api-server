use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

// ==================== Core Info Structs ====================

/// Token information with metadata and creator
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    #[serde(default)]
    pub description: Option<String>,
    pub is_listing: bool,
    #[serde(default)]
    pub twitter: Option<String>,
    #[serde(default)]
    pub telegram: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    pub created_at: i64,
    pub creator: AccountInfo,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub account_id: String,
    pub nickname: String,
    pub bio: String,
    pub image_uri: String,
    pub follower_count: i32,
    pub following_count: i32,
}

impl AccountInfo {
    pub fn new(account_id: String) -> Self {
        use rand::Rng;
        use std::env;

        let random_number = rand::thread_rng().gen_range(1..=5);
        let image_key = format!("DEFAULT_IMAGE_{}", random_number);
        let image_uri = env::var(&image_key).expect("DEFAULT_IMAGE must be set");

        Self {
            account_id: account_id.clone(),
            nickname: account_id,
            bio: "".to_string(),
            image_uri,
            follower_count: 0,
            following_count: 0,
        }
    }
}

/// Market type enum
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "VARCHAR")]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketType {
    Curve,
    Dex,
}

/// Market information with pricing data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarketInfo {
    pub market_type: MarketType,
    pub token_id: String,
    pub market_id: String,
    /// Token/USD price
    pub token_price: String,
    /// MON/USD price
    pub native_price: String,
    /// MON/Token price
    pub price: String,
    /// Total supply (used for market cap calculation in bonding curve)
    pub total_supply: String,
    /// Liquidity (used for pool liquidity in dex)
    pub liquidity: String,
    /// Volume (used for tokne total volume)
    pub volume: String,
    /// Holder count (used for tokne total holder count)
    pub holder_count: i64,
}

/// Swap event type enum
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "VARCHAR")]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwapType {
    Buy,
    Sell,
}

/// Swap transaction information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SwapInfo {
    pub event_type: SwapType,
    pub native_amount: String,
    pub token_amount: String,
    pub native_price: String,
    /// USD value at execution time
    pub value: String,
    pub transaction_hash: String,
    pub created_at: i64,
}

/// Balance information with pricing (all prices in $)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BalanceInfo {
    /// Token balance quantity
    pub balance: String,
    /// Token/USD price
    pub token_price: String,
    /// MON/USD price
    pub native_price: String,
    // holding period
    pub created_at: i64,
}

/// Token with balance information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenWithBalanceInfo {
    pub token_info: TokenInfo,
    pub balance_info: BalanceInfo,
}

/// Token with swap information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSwapInfo {
    pub token_info: TokenInfo,
    pub swap_info: SwapInfo,
}

/// Token created information with market and balance
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedInfo {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    pub balance_info: BalanceInfo,
}
