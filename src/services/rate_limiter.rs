use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use std::time::{SystemTime, UNIX_EPOCH};

/// Rate limit configuration
pub const RATE_LIMIT_WITH_API_KEY: u64 = 100;    // With API key: 100 req/min
pub const RATE_LIMIT_WITHOUT_API_KEY: u64 = 10;  // Without API key: 10 req/min

/// Rate limit check result
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: u64, limit: u64 },
    Exceeded { retry_after: u64, limit: u64 },
}

/// Check rate limit and increment counter
/// Redis key format: rate:{identifier}:min:{unix_minute}
pub async fn check_and_increment(
    redis: &RedisDatabase,
    identifier: &str,
    limit: u64,
) -> Result<RateLimitResult, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let current_minute = now / 60;
    let seconds_into_minute = now % 60;
    let retry_after = 60 - seconds_into_minute;

    let redis_key = format!("rate:{}:min:{}", identifier, current_minute);

    // Atomic INCR + EXPIRE
    let count = redis
        .incr_with_expire(&redis_key, 120) // TTL 120 seconds for safety (covers current + next minute)
        .await
        .map_err(|e| AppError::InternalError(format!("Rate limit check failed: {}", e)))?;

    if count > limit {
        return Ok(RateLimitResult::Exceeded { retry_after, limit });
    }

    let remaining = limit - count;
    Ok(RateLimitResult::Allowed { remaining, limit })
}
