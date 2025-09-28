use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct TokenMetadata {
    #[serde(rename = "token_address")]
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub twitter: String,
    pub telegram: String,
    pub website: String,
    pub is_listing: bool,
    pub created_at: i64,
    pub transaction_hash: String,
    pub creator: String,
    pub total_supply: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMetadataResponse {
    pub token_metadata: TokenMetadata,
}
