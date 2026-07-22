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
    pub is_graduated: bool,
    pub is_nsfw: bool,
    #[serde(default)]
    pub twitter: Option<String>,
    #[serde(default)]
    pub telegram: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    pub created_at: i64,
    pub creator: AccountInfo,
    pub is_cto: bool,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub account_id: String,
    pub nickname: String,
    pub bio: String,
    pub image_uri: String,
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
        }
    }
}

/// Market type enum
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq)]
#[sqlx(type_name = "VARCHAR")]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketType {
    Curve,
    Dex,
}

/// Quote token metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuoteInfo {
    pub quote_id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub image_uri: String,
}

/// Market information with pricing data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarketInfo {
    pub market_type: MarketType,
    pub token_id: String,
    pub quote_info: QuoteInfo,
    pub market_id: String,
    pub reserve_native: String,
    pub reserve_quote: String,
    pub reserve_token: String,
    /// Token/USD price
    pub token_price: String,
    /// MON/USD price (legacy; for non-native quote tokens, mirrors quote_price)
    pub native_price: String,
    /// Quote/USD price (equals native_price for native-quoted tokens)
    pub quote_price: String,
    /// MON/Token price
    pub price: String,
    /// USD/Token price
    pub price_usd: String,
    /// MON/Token price (legacy alias)
    pub price_native: String,
    /// Quote/Token price (canonical quote-denominated value)
    pub price_quote: String,
    /// Total supply (used for market cap calculation in bonding curve)
    pub total_supply: String,
    /// Volume (used for tokne total volume)
    pub volume: String,
    // Ath price(USD)
    pub ath_price: String,
    /// Ath price(USD)
    pub ath_price_usd: String,
    /// Ath price(Native) — legacy alias, equals ath_price_quote for native-quoted tokens
    pub ath_price_native: String,
    /// Ath price(Quote) — quote asset denominated ATH
    pub ath_price_quote: String,
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
    pub quote_amount: String,
    pub token_amount: String,
    pub native_price: String,
    pub quote_price: String,
    /// USD value at execution time
    pub value: String,
    pub transaction_hash: String,
    pub created_at: i64,
}

/// Balance information with pricing (all prices in $)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BalanceInfo {
    /// Wallet-held token balance
    pub balance: String,
    /// LP-locked amount of this token in the same units as `balance`. "0" when none.
    pub lp_balance: String,
    /// balance + lp_balance, in the same units. Sort key for holdings lists.
    pub total_balance: String,
    /// Token/USD price
    pub token_price: String,
    /// quote/USD price (legacy alias; 실제로는 quote_id 기준 USD)
    pub native_price: String,
    /// quote/USD price (canonical). 현재 native_price와 동일 값
    pub quote_price: String,
    // holding period (지갑 balance 기준)
    pub created_at: i64,
}

/// Token with balance information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenWithBalanceInfo {
    pub token_info: TokenInfo,
    pub balance_info: BalanceInfo,
    pub market_info: MarketInfo,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token_json() -> serde_json::Value {
        json!({
            "token_id": "0x0000000000000000000000000000000000000001",
            "name": "Token",
            "symbol": "TOK",
            "image_uri": "https://example.com/token.png",
            "description": null,
            "is_graduated": false,
            "is_nsfw": false,
            "twitter": null,
            "telegram": null,
            "website": null,
            "created_at": 1,
            "creator": {
                "account_id": "0x0000000000000000000000000000000000000002",
                "nickname": "creator",
                "bio": "",
                "image_uri": "https://example.com/account.png"
            },
            "is_cto": false,
            "x_verification": null
        })
    }

    fn market_json() -> serde_json::Value {
        json!({
            "market_type": "CURVE",
            "token_id": "0x0000000000000000000000000000000000000001",
            "quote_info": {
                "quote_id": "0x0000000000000000000000000000000000000003",
                "name": "Wrapped GIWA",
                "symbol": "WGIWA",
                "decimals": 18,
                "image_uri": "https://example.com/quote.png"
            },
            "market_id": "0x0000000000000000000000000000000000000004",
            "reserve_native": "0",
            "reserve_quote": "0",
            "reserve_token": "0",
            "token_price": "0",
            "native_price": "0",
            "quote_price": "0",
            "price": "0",
            "price_usd": "0",
            "price_native": "0",
            "price_quote": "0",
            "total_supply": "0",
            "volume": "0",
            "ath_price": "0",
            "ath_price_usd": "0",
            "ath_price_native": "0",
            "ath_price_quote": "0",
            "holder_count": 0,
            "fee_info": {
                "creator_protocol_fee_rate": 1,
                "curve_protocol_fee_rate": 2,
                "dex_protocol_fee_rate": 3
            }
        })
    }

    #[test]
    fn balance_info_serializes_lp_balance_quote_price_and_total_balance() {
        let b = BalanceInfo {
            balance: "100".into(),
            lp_balance: "5".into(),
            total_balance: "105".into(),
            token_price: "2".into(),
            native_price: "3".into(),
            quote_price: "3".into(),
            created_at: 0,
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["lp_balance"], "5");
        assert_eq!(v["total_balance"], "105");
        assert_eq!(v["quote_price"], "3");
        assert_eq!(v["quote_price"], v["native_price"]); // dual-field
    }

    #[test]
    fn market_type_rejects_versioned_variants() {
        assert_eq!(
            serde_json::from_str::<MarketType>("\"CURVE\"").unwrap(),
            MarketType::Curve
        );
        assert_eq!(
            serde_json::from_str::<MarketType>("\"DEX\"").unwrap(),
            MarketType::Dex
        );
        assert!(serde_json::from_str::<MarketType>("\"V2_CURVE\"").is_err());
        assert!(serde_json::from_str::<MarketType>("\"V2_DEX\"").is_err());
    }

    #[test]
    fn market_info_serializes_without_fee_info() {
        let info: MarketInfo = serde_json::from_value(market_json()).unwrap();
        let serialized = serde_json::to_value(info).unwrap();
        assert!(serialized.get("fee_info").is_none());
    }

    #[test]
    fn token_payloads_omit_retired_product_fields() {
        let token: TokenInfo = serde_json::from_value(token_json()).unwrap();
        let serialized_token = serde_json::to_value(token).unwrap();
        assert!(serialized_token.get("x_verification").is_none());

        let created: TokenCreatedInfo = serde_json::from_value(json!({
            "token_info": token_json(),
            "market_info": market_json(),
            "balance_info": {
                "balance": "0",
                "lp_balance": "0",
                "total_balance": "0",
                "token_price": "0",
                "native_price": "0",
                "quote_price": "0",
                "created_at": 0
            },
            "reward_info": {
                "amount": "0",
                "claimed_amount": "0",
                "proof": [],
                "claimable": false
            }
        }))
        .unwrap();
        let serialized_created = serde_json::to_value(created).unwrap();
        assert!(serialized_created.get("reward_info").is_none());
    }
}
