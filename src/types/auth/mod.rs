use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

use super::account::Account;

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "address": "Your address"
}))]
pub struct AuthNonceRequest {
    #[schema(example = "Your address")]
    pub address: String,
}
// TODO : nocne -> message 로 변경
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "nonce": "example.com wants you to sign in with your Ethereum account:\n0x0000000000000000000000000000000000000000\n\nURI: https://example.com/login\nVersion: 1\nChain ID: 1\nNonce: abced-abced-abced\nIssued At: 2023-01-01T00:00:00Z"
}))]
pub struct AuthNonceResponse {
    #[schema(
        example = "example.com wants you to sign in with your Ethereum account:\n0x0000000000000000000000000000000000000000\n\nURI: https://example.com/login\nVersion: 1\nChain ID: 1\nNonce: abced-abced-abced\nIssued At: 2023-01-01T00:00:00Z"
    )]
    pub nonce: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthSessionRequest {
    pub signature: String,
    pub nonce: String,
    pub chain_id: u64,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    pub account: Account,
}
