pub fn valid_evm_address(account_id: &str) -> bool {
    account_id.starts_with("0x")
        && account_id.len() == 42
        && account_id[2..].chars().all(|c| c.is_ascii_hexdigit())
}
