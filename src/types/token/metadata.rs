use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{MarketInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMetadataResponse {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
}
