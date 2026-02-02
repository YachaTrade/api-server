use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use std::time::{SystemTime, UNIX_EPOCH};

/// Rate limit configuration
const RATE_LIMIT_PER_MINUTE: u64 = 60;

/// Rate limit check result
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: u64 },
    Exceeded { retry_after: u64 },
}

/// Check rate limit and increment counter (60 requests per minute)
/// Redis key format: rate:{key_hash}:min:{unix_minute}
pub async fn check_and_increment(
    redis: &RedisDatabase,
    key_hash: &str,
) -> Result<RateLimitResult, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let current_minute = now / 60;
    let seconds_into_minute = now % 60;
    let retry_after = 60 - seconds_into_minute;

    let redis_key = format!("rate:{}:min:{}", key_hash, current_minute);

    // Atomic INCR + EXPIRE
    let count = redis
        .incr_with_expire(&redis_key, 120) // TTL 120 seconds for safety (covers current + next minute)
        .await
        .map_err(|e| AppError::InternalError(format!("Rate limit check failed: {}", e)))?;

    if count > RATE_LIMIT_PER_MINUTE {
        return Ok(RateLimitResult::Exceeded { retry_after });
    }

    let remaining = RATE_LIMIT_PER_MINUTE - count;
    Ok(RateLimitResult::Allowed { remaining })
}
