pub mod create_token;
pub mod metadata;
pub mod order;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::common::info::TokenInfo;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenResponse {
    pub token_info: TokenInfo,
}
