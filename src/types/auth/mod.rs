use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::common::info::AccountInfo;
use crate::utils::valid_evm_address;

// ==================== GET /auth/nonce ====================

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthNonceRequest {
    pub address: String,
}

impl AuthNonceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_evm_address(&self.address) {
            return Err("Invalid address format".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthNonceResponse {
    pub nonce: String,
}

// ==================== POST /auth/session ====================

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthSessionRequest {
    pub signature: String,
    pub nonce: String,
    pub chain_id: u64,
    pub wallet_address: Option<String>,
}

impl AuthSessionRequest {
    pub fn validate(&self) -> Result<(), String> {
        // signature format: 0x + 130 hex chars (65 bytes ECDSA)
        if !self.signature.starts_with("0x") || self.signature.len() != 132 {
            return Err("Invalid signature format".to_string());
        }
        if !self.signature[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Signature must be hexadecimal".to_string());
        }
        // nonce length limit
        if self.nonce.is_empty() || self.nonce.len() > 256 {
            return Err("Invalid nonce length".to_string());
        }
        // wallet_address validation
        if let Some(ref addr) = self.wallet_address
            && !valid_evm_address(addr)
        {
            return Err("Invalid wallet address format".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    pub account_info: AccountInfo,
}
