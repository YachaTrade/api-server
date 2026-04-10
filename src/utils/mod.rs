pub mod single_flight;

use std::str::FromStr;

use alloy::primitives::Address;

use crate::config::VANITY_ADDRESS_SUFFIX;

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
