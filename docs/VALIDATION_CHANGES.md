# Input Validation Changes

보안 점검으로 추가/수정된 입력값 검증 내역

## 신규 추가

### 1. AuthNonceRequest (`src/types/auth/mod.rs`)
```rust
pub fn validate(&self) -> Result<(), String> {
    if !valid_evm_address(&self.address) {
        return Err("Invalid address format".to_string());
    }
    Ok(())
}
```
- `address`: EVM 주소 형식 검증

### 2. AuthSessionRequest (`src/types/auth/mod.rs`)
```rust
pub fn validate(&self) -> Result<(), String> {
    // signature: 0x + 130자 hex (65 bytes ECDSA)
    if !self.signature.starts_with("0x") || self.signature.len() != 132
    if !self.signature[2..].chars().all(|c| c.is_ascii_hexdigit())
    // nonce: 1-256자
    if self.nonce.is_empty() || self.nonce.len() > 256
    // wallet_address: EVM 형식 (Optional)
    if let Some(ref addr) = self.wallet_address { valid_evm_address(addr) }
}
```

### 3. MineSaltRequest (`src/types/token/salt.rs`)
```rust
pub fn validate(&self) -> Result<(), String> {
    // creator: EVM 주소
    // name: 1-32자, 개행 금지
    // symbol: 1-10자, alphanumeric만
    // metadata_uri: ALLOWED_IMAGE_DOMAIN으로 시작
}
```

### 4. HypeVoteRequest (`src/types/hype/mod.rs`)
```rust
pub fn validate(&self) -> Result<(), String> {
    // token_id: EVM 주소 형식
    // amount: 양수 (> 0)
}
```

### 5. EventsQuery (`src/types/terminal/mod.rs`)
```rust
const MAX_BLOCK_RANGE: u64 = 10000;

pub fn validate(&self) -> Result<(), String> {
    // from_block <= to_block
    // 블록 범위 최대 10,000
}
```

### 6. RegisterWalletRequest (`src/types/account/mod.rs`)
```rust
const ALLOWED_WALLETS: &[&str] = &[
    "METAMASK", "KEPLR", "BACKPACK", "HAHA", "OKX", "PHANTOM", "RABBY", "OTHER"
];

pub fn validate(&self) -> Result<(), String> {
    // wallet: 화이트리스트 검증
}
```

### 7. SetNsfwRequest (`src/types/cms/mod.rs`)
```rust
pub fn validate(&self) -> Result<(), String> {
    // token_id: EVM 주소 형식
}
```

### 8. InsertTrendRequest (`src/types/cms/mod.rs`)
```rust
const MAX_TREND_TOKENS: usize = 50;

pub fn validate(&self) -> Result<(), String> {
    // token_ids: 최대 50개
    // 각 token_id: EVM 주소 형식
}
```

---

## 기존 수정

### 1. GetBarsRequest (`src/types/trading/chart.rs`)

**Before:**
```rust
const VALID_RESOLUTIONS: &[&str] = &["1", "5", "15", "30", "60", "240", "1D", "1W"];
```

**After:**
```rust
const VALID_RESOLUTIONS: &[&str] = &[
    "1", "5", "15", "30",
    "60", "1H",      // 1 hour
    "240", "4H",     // 4 hours
    "D", "1D",       // 1 day
    "W", "1W",       // 1 week
    "M", "1M",       // 1 month
];
```
- `resolution_to_interval_type()` 함수와 동기화

### 2. Chart countback 제한 증가 (`src/types/common/pagination.rs`)

**Before:**
```rust
pub const MAX_CHART_LIMIT: i32 = 2000;
```

**After:**
```rust
pub const MAX_CHART_LIMIT: i32 = 3000;
```

### 3. SwapQuery - trade_type (`src/types/trading/swap_history.rs`)

**Before:**
```rust
#[serde(default = "default_trade_type")]
pub trade_type: String,
```

**After:**
```rust
#[serde(default = "default_trade_type", deserialize_with = "normalize_trade_type")]
pub trade_type: String,

fn normalize_trade_type<'de, D>(deserializer: D) -> Result<String, D::Error> {
    let trade_type = String::deserialize(deserializer)?;
    Ok(trade_type.to_uppercase())  // 대소문자 정규화
}
```
- `"buy"`, `"Buy"`, `"BUY"` 모두 `"BUY"`로 정규화
- 검증은 `validate()`에서 수행

### 4. TimeFrame to_chart_interval (`src/types/trading/metrics.rs`)

**Before:**
```rust
TimeFrame::OneHour => "60",
TimeFrame::FourHours => "240",
TimeFrame::OneDay => "60",  // BUG: 1D여야 함
```

**After:**
```rust
TimeFrame::OneHour => "1H",
TimeFrame::FourHours => "4H",
TimeFrame::OneDay => "D",
```
- chart interval 형식과 일치하도록 수정

---

## Handler 호출 위치

| Request Type | Handler | 파일 |
|-------------|---------|------|
| AuthNonceRequest | `get_nonce` | `src/router/auth/handler.rs` |
| AuthSessionRequest | `create_session` | `src/router/auth/handler.rs` |
| MineSaltRequest | `mine_salt` | `src/router/token/handler.rs` |
| HypeVoteRequest | `vote` | `src/router/hype/handler.rs` |
| EventsQuery | `get_events` | `src/router/terminal/handler.rs` |
| RegisterWalletRequest | `register_wallet` | `src/router/account/handler.rs` |
| GetBarsRequest | `get_prices` | `src/router/trade/handler.rs` |
| SwapQuery | `get_token_swaps` | `src/router/trade/handler.rs` |
| SetNsfwRequest | `set_nsfw` | `src/router/cms/handler.rs` |
| InsertTrendRequest | `insert_trend` | `src/router/cms/handler.rs` |
