use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Database model for API keys
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: i64,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub request_count: i64,
}

impl ApiKey {
    /// Check if the API key is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        if !self.is_active {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            return Utc::now() < expires_at;
        }
        true
    }
}

/// Cached API key info stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedApiKey {
    pub id: i64,
    pub key_hash: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request to create a new API key
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner_address: Option<String>,
    /// Expiration in days (None = never expires)
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

/// Response when creating a new API key
/// NOTE: api_key is only returned once at creation time
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub id: i64,
    /// Full API key - ONLY returned at creation time, store securely!
    pub api_key: String,
    pub key_prefix: String,
    pub name: String,
}

/// API key info for listing (without sensitive data)
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_address: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub request_count: i64,
}

impl From<ApiKey> for ApiKeyInfo {
    fn from(key: ApiKey) -> Self {
        Self {
            id: key.id,
            key_prefix: key.key_prefix,
            name: key.name,
            description: key.description,
            owner_address: key.owner_address,
            is_active: key.is_active,
            created_at: key.created_at,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            request_count: key.request_count,
        }
    }
}

/// API key list response
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyListResponse {
    pub api_keys: Vec<ApiKeyInfo>,
    pub total: i64,
}
