use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::MarketInfo;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarketResponse {
    pub market_info: MarketInfo,
}
