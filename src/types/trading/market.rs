use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Market {
    pub market_type: String,
    pub token_id: String,
    pub market_id: Option<String>,
    pub price: String,
    pub total_supply: String,
}
