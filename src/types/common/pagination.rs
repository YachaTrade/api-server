use serde::{Deserialize, Deserializer};
use utoipa::ToSchema;

/// Maximum limit for general pagination
pub const MAX_LIMIT: i64 = 100;
/// Default limit for pagination
pub const DEFAULT_LIMIT: i64 = 10;
/// Maximum limit for chart data
pub const MAX_CHART_LIMIT: i32 = 3000;
/// Default limit for chart data
pub const DEFAULT_CHART_LIMIT: i32 = 500;

/// Validate and deserialize limit (1 to MAX_LIMIT)
pub fn deserialize_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i64::deserialize(deserializer)?;
    if value < 1 || value > MAX_LIMIT {
        Err(serde::de::Error::custom(format!(
            "Invalid limit: must be between 1 and {}",
            MAX_LIMIT
        )))
    } else {
        Ok(value)
    }
}

/// Validate and deserialize chart countback (1 to MAX_CHART_LIMIT)
pub fn deserialize_chart_limit<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i32::deserialize(deserializer)?;
    if value < 1 || value > MAX_CHART_LIMIT {
        Err(serde::de::Error::custom(format!(
            "Invalid countback: must be between 1 and {}",
            MAX_CHART_LIMIT
        )))
    } else {
        Ok(value)
    }
}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

/// Validate and deserialize page (minimum 1)
pub fn deserialize_page<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i64::deserialize(deserializer)?;
    if value < 1 {
        Err(serde::de::Error::custom("Invalid page: must be at least 1"))
    } else {
        Ok(value)
    }
}

/// Validate and deserialize offset (non-negative)
pub fn deserialize_offset<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i64::deserialize(deserializer)?;
    if value < 0 {
        Err(serde::de::Error::custom(
            "Invalid offset: must be non-negative",
        ))
    } else {
        Ok(value)
    }
}

#[derive(Clone, Deserialize, ToSchema, Debug)]
pub struct PaginationParams {
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
    #[serde(default = "default_direction", deserialize_with = "validate_direction")]
    pub direction: String,
}

pub fn default_direction() -> String {
    "DESC".to_string()
}

pub fn default_page() -> i64 {
    1
}

pub fn validate_direction<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let direction = String::deserialize(deserializer)?;
    let direction_upper = direction.to_uppercase();
    if direction_upper != "DESC" && direction_upper != "ASC" {
        Err(serde::de::Error::custom(
            "Direction must be either DESC or ASC",
        ))
    } else {
        Ok(direction_upper)
    }
}
