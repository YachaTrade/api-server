use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::token::x_verification::TokenXVerification;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XInfoResponse {
    pub x_verification: Option<TokenXVerification>,
}
