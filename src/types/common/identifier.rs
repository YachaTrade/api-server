use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum Identifier {
    Nickname(String),
    Address(String), // Ethereum address
}

impl Identifier {
    /// Check if the identifier is an Ethereum address
    pub fn is_address(value: &str) -> bool {
        value.starts_with("0x") && value.len() == 42
    }

    /// Get the underlying string value
    pub fn value(&self) -> &str {
        match self {
            Identifier::Nickname(nick) => nick,
            Identifier::Address(addr) => addr,
        }
    }
}

impl From<String> for Identifier {
    fn from(value: String) -> Self {
        if Self::is_address(&value) {
            Identifier::Address(value)
        } else {
            Identifier::Nickname(value)
        }
    }
}

impl From<&str> for Identifier {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}
