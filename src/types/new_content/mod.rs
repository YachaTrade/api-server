use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{AccountInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewContentResponse {
    pub new_buy: Option<NewSwapMessage>,
    pub new_sell: Option<NewSwapMessage>,
    pub new_token: Option<NewTokenMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewSwapMessage {
    pub account_info: AccountInfo,
    pub token_info: TokenInfo,
    pub is_buy: bool,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewTokenMessage {
    pub account_info: AccountInfo,
    pub token_info: TokenInfo,
}
