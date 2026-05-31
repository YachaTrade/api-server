pub mod single_flight;

use std::str::FromStr;

use alloy::primitives::Address;
use tracing::warn;

use crate::{config::VANITY_ADDRESS_SUFFIX, result::AppError, state::AppState};

/// Account ID 검증: EVM 주소 형식 확인 후 체크섬 주소 반환
pub fn valid_account_id(address: &str) -> Option<String> {
    Address::from_str(address)
        .map(|addr| addr.to_checksum(None))
        .ok()
}

/// Token ID 검증: EVM 주소 형식 + VANITY_ADDRESS_SUFFIX 확인 후 체크섬 주소 반환
pub fn valid_token_id(token_id: &str) -> Option<String> {
    if !token_id
        .to_lowercase()
        .ends_with(&VANITY_ADDRESS_SUFFIX.to_lowercase())
    {
        return None;
    }
    valid_account_id(token_id)
}

/// Token ID 검증 + DB 존재 확인.
///
/// 1) `valid_token_id`로 형식/체크섬 검증 → 실패 시 400 BadRequest
/// 2) Redis `token_exists:<id>` 캐시 hit → 통과 (1주일 TTL)
/// 3) 캐시 miss → `SELECT EXISTS(SELECT 1 FROM token WHERE token_id = $1)`
///    - exists=true → 캐시에 1주일 박고 통과
///    - exists=false → **404 NotFound** (캐싱 안 함 — 새 token 가시성 위해)
/// 4) Redis 또는 DB 에러 → fail-open (요청 통과). 인프라 장애로 정상 트래픽
///    막히는 일은 만들지 않음. warn! 로그만 찍힘.
pub async fn valid_existing_token_id(state: &AppState, token_id: &str) -> Result<String, AppError> {
    let token_id = valid_token_id(token_id)
        .ok_or_else(|| AppError::BadRequest("Invalid token id".to_string()))?;

    // Cache hit → 통과
    if let Ok(true) = state.redis.get_token_exists(&token_id).await {
        return Ok(token_id);
    }

    // Cache miss or read error → DB에 묻기 (단일 EXISTS 쿼리, index lookup)
    let exists_result: Result<bool, sqlx::Error> =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM token WHERE token_id = $1)")
            .bind(&token_id)
            .fetch_one(state.postgres.get_read_pool())
            .await;

    match exists_result {
        Ok(true) => {
            // Positive 캐시 (1주일). set 실패해도 요청 자체는 진행.
            if let Err(e) = state.redis.set_token_exists(&token_id).await {
                warn!("set_token_exists failed for {}: {}", token_id, e);
            }
            Ok(token_id)
        }
        Ok(false) => Err(AppError::NotFound(format!("Token not found: {}", token_id))),
        Err(e) => {
            // DB 에러 → fail-open: 요청은 통과시키고 후속 핸들러가 자기 식대로 처리하게 함.
            warn!(
                "token existence check DB error for {}: {} — failing open",
                token_id, e
            );
            Ok(token_id)
        }
    }
}

pub fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn calculate_price_change_percent(start_price: &str, current_price: &str) -> Option<f64> {
    let start: f64 = start_price.parse().ok()?;
    let current: f64 = current_price.parse().ok()?;

    if (start - 0.0).abs() < f64::EPSILON {
        return None;
    }

    let change_percent = ((current - start) / start) * 100.0;
    // Round to 3 decimal places
    Some((change_percent * 1000.0).round() / 1000.0)
}
