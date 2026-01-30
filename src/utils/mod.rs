pub mod single_flight;

use std::str::FromStr;

use alloy::primitives::Address;

use crate::config::VANITY_ADDRESS_SUFFIX;

pub fn valid_evm_address(address: &str) -> bool {
    Address::from_str(address).is_ok()
}

/// EVM 주소를 검증하고 체크섬된 주소로 반환
pub fn normalize_evm_address(address: &str) -> Option<String> {
    Address::from_str(address)
        .map(|addr| addr.to_checksum(None))
        .ok()
}

/// 토큰 ID 검증: EVM 주소 형식 + VANITY_ADDRESS_SUFFIX로 끝나는지 확인 후 체크섬 주소 반환
pub fn validate_token_id(token_id: &str) -> Option<String> {
    if !token_id.to_lowercase().ends_with(&VANITY_ADDRESS_SUFFIX.to_lowercase()) {
        return None;
    }

    normalize_evm_address(token_id)
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
