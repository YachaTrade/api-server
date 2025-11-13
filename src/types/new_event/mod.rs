use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::{AccountInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Buy,
    Sell,
    Create,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewEvent {
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub amount: String,
    pub token_info: TokenInfo,
    pub account_info: AccountInfo,
    #[serde(skip)]
    pub event_created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewEventResponse {
    pub new_events: Vec<NewEvent>,
}
