use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Aggregate X-verification signals for one coin. The creator's own X handle
/// and X user-id are NEVER included here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TokenXVerification {
    pub followers_count: i64,
    pub followed_by: Vec<XFollowedByEntry>,
}

/// A third party that provably follows the creator (public by design).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct XFollowedByEntry {
    pub x_handle: String,
    pub x_image_uri: String,
    pub x_followers_count: i64,
    pub is_x_verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_nested_followed_by() {
        let v = TokenXVerification {
            followers_count: 128_000,
            followed_by: vec![XFollowedByEntry {
                x_handle: "elonmusk".into(),
                x_image_uri: "https://img".into(),
                x_followers_count: 200_000_000,
                is_x_verified: true,
            }],
        };
        let j = serde_json::to_value(&v).unwrap();
        assert_eq!(j["followers_count"], 128_000);
        assert_eq!(j["followed_by"][0]["x_handle"], "elonmusk");
        assert_eq!(j["followed_by"][0]["is_x_verified"], true);
    }
}
