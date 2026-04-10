use crate::{
    types::common::{
        info::{AccountInfo, TokenInfo},
        pagination::{
            DEFAULT_LIMIT, default_direction, default_page, deserialize_limit, deserialize_page,
            validate_direction,
        },
    },
    utils::valid_account_id,
};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;

/// Volume range filter for swap history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VolumeRange {
    /// $0 ~ $1000
    Small,
    /// $1000 ~ $10000
    Medium,
    /// $10000+
    Large,
}

impl VolumeRange {
    /// Get the minimum value for this range
    pub fn min_value(&self) -> &str {
        match self {
            VolumeRange::Small => "0",
            VolumeRange::Medium => "1000",
            VolumeRange::Large => "10000",
        }
    }

    /// Get the maximum value for this range (None means no upper limit)
    pub fn max_value(&self) -> Option<&str> {
        match self {
            VolumeRange::Small => Some("1000"),
            VolumeRange::Medium => Some("10000"),
            VolumeRange::Large => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionSwap {
    pub token: TokenInfo,
    pub account_id: String,
    pub is_buy: bool,
    pub native_amount: String,
    pub token_amount: String,
    pub created_at: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionSwapResponse {
    pub swaps: Vec<PositionSwap>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSwap {
    pub account_info: AccountInfo,
    pub swap_info: crate::types::common::info::SwapInfo,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSwapResponse {
    pub swaps: Vec<TokenSwap>,
    pub total_count: i64,
}

fn default_swap_limit() -> i64 {
    DEFAULT_LIMIT
}

/// Combined query parameters for swap history
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct SwapQuery {
    // PaginationParams fields
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    #[serde(default = "default_swap_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
    #[serde(default = "default_direction", deserialize_with = "validate_direction")]
    pub direction: String,

    /// Volume range filters - can select multiple ranges (e.g., ?volume_ranges=small&volume_ranges=large)
    #[serde(default, deserialize_with = "deserialize_volume_ranges")]
    pub volume_ranges: Option<Vec<VolumeRange>>,

    /// Account ID for own trades filter
    #[serde(default)]
    pub account_id: Option<String>,

    /// Trade type filter: "BUY", "SELL", or "ALL" (default)
    #[serde(
        default = "default_trade_type",
        deserialize_with = "normalize_trade_type"
    )]
    pub trade_type: String,
}

impl SwapQuery {
    /// Validate the query parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.is_some()
            && valid_account_id(self.account_id.as_ref().unwrap()).is_none()
        {
            return Err("Invalid account ID format".to_string());
        }

        // Validate trade_type
        if !["BUY", "SELL", "ALL"].contains(&self.trade_type.as_str()) {
            return Err("Invalid trade_type: must be 'BUY', 'SELL', or 'ALL'".to_string());
        }

        Ok(())
    }
}

fn default_trade_type() -> String {
    "ALL".to_string()
}

/// Deserialize trade_type, normalizing to uppercase
fn normalize_trade_type<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let trade_type = String::deserialize(deserializer)?;
    Ok(trade_type.to_uppercase())
}

fn deserialize_volume_ranges<'de, D>(deserializer: D) -> Result<Option<Vec<VolumeRange>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) => {
            let mut ranges = Vec::new();
            for part in s.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    let range = match trimmed.to_lowercase().as_str() {
                        "small" => VolumeRange::Small,
                        "medium" => VolumeRange::Medium,
                        "large" => VolumeRange::Large,
                        _ => {
                            return Err(Error::custom(format!(
                                "Invalid volume range: '{}'. Valid values: small, medium, large",
                                trimmed
                            )));
                        }
                    };
                    ranges.push(range);
                }
            }
            Ok(if ranges.is_empty() {
                None
            } else {
                Some(ranges)
            })
        }
    }
}
