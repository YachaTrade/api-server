use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::QuoteInfo;

/// Response for `GET /quote_token` — full list of registered quote tokens.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuoteTokensResponse {
    pub quote_tokens: Vec<QuoteInfo>,
}
