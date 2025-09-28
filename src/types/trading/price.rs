use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PriceResponse {
    pub price: String,
    pub token_address: String,
}
