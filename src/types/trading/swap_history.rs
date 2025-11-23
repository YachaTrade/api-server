use crate::{
    types::common::info::{AccountInfo, TokenInfo},
    utils::valid_evm_address,
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

/// Combined query parameters for swap history
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct SwapQuery {
    // PaginationParams fields
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "validate_limit")]
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
    #[serde(default = "default_trade_type")]
    pub trade_type: String,
}

impl SwapQuery {
    /// Validate the query parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.is_some() {
            if !valid_evm_address(self.account_id.as_ref().unwrap()) {
                return Err("Invalid account ID format".to_string());
            }
        }

        // Validate trade_type
        if !["BUY", "SELL", "ALL"].contains(&self.trade_type.as_str()) {
            return Err("Invalid trade_type: must be 'BUY', 'SELL', or 'ALL'".to_string());
        }

        Ok(())
    }
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    10
}

fn default_direction() -> String {
    "DESC".to_string()
}

fn default_trade_type() -> String {
    "ALL".to_string()
}

fn validate_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if limit < 1 || limit > 100 {
        return Err(serde::de::Error::custom(
            "Invalid limit: must be between 1 and 100",
        ));
    }
    Ok(limit)
}

fn validate_direction<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let direction = String::deserialize(deserializer)?;
    let direction_upper = direction.to_uppercase();
    if !["ASC", "DESC"].contains(&direction_upper.as_str()) {
        return Err(serde::de::Error::custom(
            "Invalid direction: must be 'ASC' or 'DESC'",
        ));
    }
    Ok(direction_upper)
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
                            )))
                        }
                    };
                    ranges.push(range);
                }
            }
            Ok(if ranges.is_empty() { None } else { Some(ranges) })
        }
    }
}
