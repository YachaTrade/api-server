use crate::db::postgres::PostgresDatabase;
use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use crate::types::api_key::{ApiKey, CachedApiKey, CreateApiKeyRequest, CreateApiKeyResponse};
use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};

const API_KEY_PREFIX: &str = "nadfun_";
const API_KEY_LENGTH: usize = 32;
const API_KEY_CACHE_TTL_SECS: u64 = 300; // 5 minutes
const MAX_API_KEYS_PER_ACCOUNT: i64 = 5;

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
    // Check API key limit per account
    if let Some(ref owner) = req.owner_address {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_keys WHERE owner_address = $1",
        )
        .bind(owner)
        .fetch_one(db.get_read_pool())
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to count API keys: {}", e)))?;

        if count >= MAX_API_KEYS_PER_ACCOUNT {
            return Err(AppError::BadRequest(format!(
                "Maximum {} API keys per account. Please delete an existing key first.",
                MAX_API_KEYS_PER_ACCOUNT
            )));
        }
    }

    let api_key = generate_api_key();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = api_key[..12].to_string(); // "nad_" + 8 chars

    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(days));

    let id: i64 = sqlx::query_scalar(
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

const INVALID_API_KEY_MSG: &str = "Invalid or expired API key";

/// Validate an API key and return its info
pub async fn validate_api_key(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    api_key: &str,
) -> Result<CachedApiKey, AppError> {
    // Validate format (use generic error message to avoid information leakage)
    if !api_key.starts_with(API_KEY_PREFIX) || api_key.len() != API_KEY_PREFIX.len() + API_KEY_LENGTH
    {
        return Err(AppError::Unauthorized(INVALID_API_KEY_MSG.to_string()));
    }

    let key_hash = hash_api_key(api_key);

    // Check Redis cache first
    if let Ok(Some(cached)) = redis.get_cached_api_key(&key_hash).await {
        if let Ok(info) = serde_json::from_str::<CachedApiKey>(&cached) {
            if let Some(expires_at) = info.expires_at {
                if Utc::now() >= expires_at {
                    return Err(AppError::Unauthorized(INVALID_API_KEY_MSG.to_string()));
                }
            }
            return Ok(info);
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
    .ok_or_else(|| AppError::Unauthorized(INVALID_API_KEY_MSG.to_string()))?;

    if !api_key_record.is_valid() {
        return Err(AppError::Unauthorized(INVALID_API_KEY_MSG.to_string()));
    }

    // Cache the result
    let cached = CachedApiKey {
        id: api_key_record.id,
        key_hash: api_key_record.key_hash.clone(),
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

/// Update usage in Redis (fast, in-memory) - called on every request
/// DB sync happens periodically via sync_api_key_usage_to_db
pub async fn update_last_used(redis: &RedisDatabase, key_hash: &str) {
    // Increment usage counter in Redis (atomic, fast)
    let _ = redis.incr_api_key_usage(key_hash).await;
    // Update last_used timestamp in Redis
    let _ = redis.set_api_key_last_used(key_hash).await;
}

/// Sync API key usage counts from Redis to DB (call periodically, e.g., every 5 minutes)
/// Returns the number of keys synced
pub async fn sync_api_key_usage_to_db(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
) -> Result<usize, AppError> {
    // Get all usage keys from Redis
    let keys = redis
        .get_all_api_key_usage_keys()
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to get usage keys: {}", e)))?;

    let mut synced_count = 0;

    for redis_key in keys {
        // Extract key_hash from "apikey:usage:{key_hash}"
        let key_hash = match redis_key.strip_prefix("apikey:usage:") {
            Some(hash) => hash,
            None => continue,
        };

        // Get and reset count atomically
        let count = redis
            .get_and_reset_api_key_usage(key_hash)
            .await
            .unwrap_or(0);

        if count > 0 {
            // Get last_used timestamp from Redis
            let last_used = redis.get_api_key_last_used(key_hash).await.ok().flatten();

            // Update DB with accumulated count
            let query = if let Some(ts) = last_used {
                let last_used_at =
                    chrono::DateTime::from_timestamp(ts, 0).unwrap_or_else(chrono::Utc::now);
                sqlx::query(
                    "UPDATE api_keys SET request_count = request_count + $1, last_used_at = $2 WHERE key_hash = $3",
                )
                .bind(count)
                .bind(last_used_at)
                .bind(key_hash)
            } else {
                sqlx::query(
                    "UPDATE api_keys SET request_count = request_count + $1, last_used_at = NOW() WHERE key_hash = $2",
                )
                .bind(count)
                .bind(key_hash)
            };

            if query.execute(db.get_write_pool()).await.is_ok() {
                synced_count += 1;
            }
        }
    }

    Ok(synced_count)
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

/// Delete an API key (admin) - hard delete
pub async fn delete_api_key(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    id: i64,
) -> Result<(), AppError> {
    let result: Option<(String,)> = sqlx::query_as(
        "DELETE FROM api_keys WHERE id = $1 RETURNING key_hash",
    )
    .bind(id)
    .fetch_optional(db.get_write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if let Some((key_hash,)) = result {
        // Invalidate cache and cleanup usage data
        let _ = redis.delete_cached_api_key(&key_hash).await;
        let _ = redis.delete_api_key_usage_data(&key_hash).await;
        Ok(())
    } else {
        Err(AppError::NotFound("API key not found".to_string()))
    }
}

/// Delete an API key by owner (user can only delete their own keys) - hard delete
pub async fn delete_api_key_by_owner(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    id: i64,
    owner_address: &str,
) -> Result<(), AppError> {
    let result: Option<(String,)> = sqlx::query_as(
        "DELETE FROM api_keys WHERE id = $1 AND owner_address = $2 RETURNING key_hash",
    )
    .bind(id)
    .bind(owner_address)
    .fetch_optional(db.get_write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if let Some((key_hash,)) = result {
        // Invalidate cache and cleanup usage data
        let _ = redis.delete_cached_api_key(&key_hash).await;
        let _ = redis.delete_api_key_usage_data(&key_hash).await;
        Ok(())
    } else {
        Err(AppError::NotFound(
            "API key not found or not owned by user".to_string(),
        ))
    }
}
