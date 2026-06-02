pub mod balance;
pub mod pyth;

use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use bigdecimal::num_bigint::BigInt;
use bigdecimal::BigDecimal;

/// 토큰별 USD 가격 소스 (운영=Pyth Hermes, 테스트=fake).
#[async_trait]
pub trait PriceSource: Send + Sync {
    /// feed_id → USD 가격. 조회 실패한 id는 맵에서 누락(부분 성공 허용).
    async fn prices_usd(&self, feed_ids: &[String]) -> HashMap<String, BigDecimal>;
}

/// 토큰별 온체인 잔액 소스 (운영=RPC balanceOf, 테스트=fake).
#[async_trait]
pub trait BalanceSource: Send + Sync {
    /// raw wei 잔액. 조회 실패 시 None(graceful degrade).
    async fn balance_of(&self, token_id: &str, account: &str) -> Option<BigDecimal>;
}

/// balance_usd = balance / 10^decimals × price_usd.
pub fn compute_balance_usd(
    balance_wei: &BigDecimal,
    decimals: i32,
    price_usd: &BigDecimal,
) -> BigDecimal {
    let scale = BigDecimal::new(BigInt::from(1), -(decimals as i64)); // 10^decimals
    (balance_wei / scale) * price_usd
}

/// Hermes `/v2/updates/price/latest` 응답에서 feed_id → USD 가격 추출.
/// price = mantissa × 10^expo. 반환 키는 0x 없는 소문자 hex.
pub fn parse_hermes_prices(body: &str) -> HashMap<String, BigDecimal> {
    let mut out = HashMap::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return out;
    };
    let Some(parsed) = json.get("parsed").and_then(|p| p.as_array()) else {
        return out;
    };
    for item in parsed {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let price = item.get("price");
        let Some(mantissa) = price
            .and_then(|p| p.get("price"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let Some(expo) = price
            .and_then(|p| p.get("expo"))
            .and_then(|v| v.as_i64())
        else {
            continue;
        };
        let Ok(mantissa) = BigInt::from_str(mantissa) else {
            continue;
        };
        // value = mantissa × 10^expo = BigDecimal::new(mantissa, -expo)
        let value = BigDecimal::new(mantissa, -expo);
        out.insert(normalize_feed_id(id), value);
    }
    out
}

/// 0x 접두사 제거 + 소문자화 (요청/응답 id 매칭 정규화).
pub fn normalize_feed_id(id: &str) -> String {
    id.trim_start_matches("0x")
        .trim_start_matches("0X")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_usd_scales_by_decimals_and_price() {
        // 2 토큰(18dp) × $1.5 = $3
        let balance = BigDecimal::from_str("2000000000000000000").unwrap();
        let price = BigDecimal::from_str("1.5").unwrap();
        let got = compute_balance_usd(&balance, 18, &price);
        assert_eq!(got, BigDecimal::from_str("3.0").unwrap());
    }

    #[test]
    fn parse_hermes_applies_expo() {
        // price 100000000 × 10^-8 = 1.0
        let body = r#"{"parsed":[{"id":"AbCd","price":{"price":"100000000","expo":-8}}]}"#;
        let m = parse_hermes_prices(body);
        assert_eq!(
            m.get("abcd"),
            Some(&BigDecimal::from_str("1.00000000").unwrap())
        );
    }

    #[test]
    fn parse_hermes_empty_on_garbage() {
        assert!(parse_hermes_prices("not json").is_empty());
        assert!(parse_hermes_prices(r#"{"parsed":[]}"#).is_empty());
    }
}
