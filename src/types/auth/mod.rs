use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::common::info::AccountInfo;

// ==================== GET /auth/nonce ====================

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthNonceRequest {
    pub address: String,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    pub account_info: AccountInfo,
}
