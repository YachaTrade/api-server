pub mod single_flight;

pub fn valid_evm_address(account_id: &str) -> bool {
    account_id.starts_with("0x")
        && account_id.len() == 42
        && account_id[2..].chars().all(|c| c.is_ascii_hexdigit())
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
