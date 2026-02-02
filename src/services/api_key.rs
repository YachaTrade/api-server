use crate::db::postgres::PostgresDatabase;
use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use crate::types::api_key::{ApiKey, CachedApiKey, CreateApiKeyRequest, CreateApiKeyResponse};
use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const API_KEY_PREFIX: &str = "nadfun_";
const API_KEY_LENGTH: usize = 32;
const API_KEY_CACHE_TTL_SECS: u64 = 300; // 5 minutes

/// Generate a new API key with prefix
pub fn generate_api_key() -> String {
    let random_part: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(API_KEY_LENGTH)
        .map(char::from)
        .collect();
    format!("{}{}", API_KEY_PREFIX, random_part)
}

/// Hash an API key using SHA256
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Create a new API key
pub async fn create_api_key(
    db: &PostgresDatabase,
    req: CreateApiKeyRequest,
) -> Result<CreateApiKeyResponse, AppError> {
    let api_key = generate_api_key();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = api_key[..12].to_string(); // "nad_" + 8 chars

    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(days));

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO api_keys (key_hash, key_prefix, name, description, owner_address, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.owner_address)
    .bind(expires_at)
    .fetch_one(db.get_write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Failed to create API key: {}", e)))?;

    Ok(CreateApiKeyResponse {
        id,
        api_key, // Only time this is returned!
        key_prefix,
        name: req.name,
    })
}

/// Validate an API key and return its info
pub async fn validate_api_key(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    api_key: &str,
) -> Result<CachedApiKey, AppError> {
    // Validate format
    if !api_key.starts_with(API_KEY_PREFIX) || api_key.len() != API_KEY_PREFIX.len() + API_KEY_LENGTH
    {
        return Err(AppError::Unauthorized(
            "Invalid API key format".to_string(),
        ));
    }

    let key_hash = hash_api_key(api_key);

    // Check Redis cache first
    if let Ok(Some(cached)) = redis.get_cached_api_key(&key_hash).await {
        if let Ok(info) = serde_json::from_str::<CachedApiKey>(&cached) {
            if info.is_active {
                if let Some(expires_at) = info.expires_at {
                    if Utc::now() >= expires_at {
                        return Err(AppError::Unauthorized("API key expired".to_string()));
                    }
                }
                return Ok(info);
            }
            return Err(AppError::Unauthorized("API key is inactive".to_string()));
        }
    }

    // Cache miss - query database
    let api_key_record: ApiKey = sqlx::query_as(
        r#"
        SELECT id, key_hash, key_prefix, name, description, owner_address,
               created_at, expires_at, last_used_at, is_active, request_count
        FROM api_keys
        WHERE key_hash = $1
        "#,
    )
    .bind(&key_hash)
    .fetch_optional(db.get_read_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
    .ok_or_else(|| AppError::Unauthorized("API key not found".to_string()))?;

    if !api_key_record.is_valid() {
        return Err(AppError::Unauthorized(
            "API key is invalid or expired".to_string(),
        ));
    }

    // Cache the result
    let cached = CachedApiKey {
        id: api_key_record.id,
        key_hash: api_key_record.key_hash.clone(),
        is_active: api_key_record.is_active,
        expires_at: api_key_record.expires_at,
    };

    let _ = redis
        .cache_api_key(
            &key_hash,
            &serde_json::to_string(&cached).unwrap(),
            API_KEY_CACHE_TTL_SECS,
        )
        .await;

    Ok(cached)
}

/// Update last_used_at timestamp (fire and forget)
pub async fn update_last_used(db: &PostgresDatabase, key_hash: &str) {
    let _ = sqlx::query(
        "UPDATE api_keys SET last_used_at = NOW(), request_count = request_count + 1 WHERE key_hash = $1",
    )
    .bind(key_hash)
    .execute(db.get_write_pool())
    .await;
}

/// List all API keys (admin)
pub async fn list_api_keys(db: &PostgresDatabase) -> Result<Vec<ApiKey>, AppError> {
    sqlx::query_as(
        r#"
        SELECT id, key_hash, key_prefix, name, description, owner_address,
               created_at, expires_at, last_used_at, is_active, request_count
        FROM api_keys
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db.get_read_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))
}

/// List API keys by owner address
pub async fn list_api_keys_by_owner(
    db: &PostgresDatabase,
    owner_address: &str,
) -> Result<Vec<ApiKey>, AppError> {
    sqlx::query_as(
        r#"
        SELECT id, key_hash, key_prefix, name, description, owner_address,
               created_at, expires_at, last_used_at, is_active, request_count
        FROM api_keys
        WHERE owner_address = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(owner_address)
    .fetch_all(db.get_read_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))
}

/// Revoke (deactivate) an API key (admin)
pub async fn revoke_api_key(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    id: Uuid,
) -> Result<(), AppError> {
    let result: Option<(String,)> = sqlx::query_as(
        "UPDATE api_keys SET is_active = FALSE WHERE id = $1 RETURNING key_hash",
    )
    .bind(id)
    .fetch_optional(db.get_write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if let Some((key_hash,)) = result {
        // Invalidate cache
        let _ = redis.delete_cached_api_key(&key_hash).await;
        Ok(())
    } else {
        Err(AppError::NotFound("API key not found".to_string()))
    }
}

/// Revoke (deactivate) an API key by owner (user can only revoke their own keys)
pub async fn revoke_api_key_by_owner(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    id: Uuid,
    owner_address: &str,
) -> Result<(), AppError> {
    let result: Option<(String,)> = sqlx::query_as(
        "UPDATE api_keys SET is_active = FALSE WHERE id = $1 AND owner_address = $2 RETURNING key_hash",
    )
    .bind(id)
    .bind(owner_address)
    .fetch_optional(db.get_write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if let Some((key_hash,)) = result {
        // Invalidate cache
        let _ = redis.delete_cached_api_key(&key_hash).await;
        Ok(())
    } else {
        Err(AppError::NotFound(
            "API key not found or not owned by user".to_string(),
        ))
    }
}
