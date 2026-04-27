use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::TokenVersion;
use crate::types::metadata::{
    MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH, MIN_NAME_LENGTH, MIN_SYMBOL_LENGTH,
};
use crate::utils::valid_account_id;

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

    /// Bonding curve version ("V1" or "V2", defaults to "V1")
    #[serde(default = "default_version")]
    #[schema(example = "V1")]
    pub version: TokenVersion,
}

fn default_version() -> TokenVersion {
    TokenVersion::V1
}

impl MineSaltRequest {
    pub fn validate(&self) -> Result<(), String> {
        // creator address validation
        if valid_account_id(&self.creator).is_none() {
            return Err("Invalid creator address format".to_string());
        }
        // name validation
        if self.name.len() < MIN_NAME_LENGTH || self.name.len() > MAX_NAME_LENGTH {
            return Err(format!(
                "Name must be {}-{} characters",
                MIN_NAME_LENGTH, MAX_NAME_LENGTH
            ));
        }
        if self.name.contains('\n') || self.name.contains('\r') {
            return Err("Name cannot contain newlines".to_string());
        }
        // symbol validation
        if self.symbol.len() < MIN_SYMBOL_LENGTH || self.symbol.len() > MAX_SYMBOL_LENGTH {
            return Err(format!(
                "Symbol must be {}-{} characters",
                MIN_SYMBOL_LENGTH, MAX_SYMBOL_LENGTH
            ));
        }
        if !self.symbol.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("Symbol must be alphanumeric".to_string());
        }
        // metadata_uri domain validation
        let allowed_domain = std::env::var("ALLOWED_IMAGE_DOMAIN")
            .unwrap_or_else(|_| "https://storage.nadapp.net/".to_string());
        if !self.metadata_uri.starts_with(&allowed_domain) {
            return Err(format!(
                "Invalid metadata URI domain, must start with {}",
                allowed_domain
            ));
        }
        Ok(())
    }
}

/// Response containing the mined salt and resulting address
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MineSaltResponse {
    /// The salt value (as 0x-prefixed hex string) that produces the desired address
    #[schema(example = "0x000000000000000000000000000000000000000000000000000000000000a3f5")]
    pub salt: String,

    /// The resulting token address (ending with the desired suffix)
    #[schema(example = "0x742d35Cc6634C0532925a3b844Bc9e7595f7777")]
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
