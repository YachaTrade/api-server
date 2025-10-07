use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{AccountInfo, BalanceInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolder {
    pub account_info: AccountInfo,
    pub balance_info: BalanceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolderResponse {
    pub holders: Vec<TokenHolder>,
    pub total_count: i64,
}
