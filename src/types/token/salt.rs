use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request parameters for mining a salt to generate a vanity token address
/// Matches Solidity's TokenCreationParams struct
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MineSaltRequest {
    /// Creator of the token
    #[schema(example = "0x742d35Cc6634C0532925a3b844Bc9e7595f70143")]
    pub creator: String,

    /// Token name
    #[schema(example = "My Token")]
    pub name: String,

    /// Token symbol
    #[schema(example = "MTK")]
    pub symbol: String,

    /// Token metadata URI
    #[schema(
        example = "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json"
    )]
    pub metadata_uri: String,
}

/// Response containing the mined salt and resulting address
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MineSaltResponse {
    /// The salt value (as 0x-prefixed hex string) that produces the desired address
    #[schema(example = "0x000000000000000000000000000000000000000000000000000000000000a3f5")]
    pub salt: String,

    /// The resulting token address (ending with the desired suffix)
    #[schema(example = "0x742d35Cc6634C0532925a3b844Bc9e7595f0143")]
    pub address: String,
}

/// Error response when salt mining fails or times out
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MineSaltError {
    /// Error message describing what went wrong
    pub error: String,

    /// Number of iterations attempted before giving up
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations_attempted: Option<u64>,
}
