use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Clone, Deserialize, ToSchema, Debug)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "validate_limit")]
    pub limit: i64,
    #[serde(default = "default_direction", deserialize_with = "validate_direction")]
    pub direction: String,
}

impl PaginationParams {
    // 페이지 번호가 음수인 경우 역순 정렬을 의미
    pub fn is_reverse_order(&self) -> bool {
        self.page < 0
    }
}

fn default_direction() -> String {
    "DESC".to_string()
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    10
}

fn validate_direction<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
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

fn validate_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if limit > 100 {
        Err(serde::de::Error::custom("Limit must be 100 or less"))
    } else {
        Ok(limit)
    }
}
