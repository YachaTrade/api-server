use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use moka::future::Cache;
use once_cell::sync::Lazy;

use crate::services::pricing::PriceSource;
use crate::utils::single_flight::with_cache;

/// 배치 응답(body) 캐시 (15초 TTL). 키 = chain + 정렬된 주소 목록.
static PRICE_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(15))
        .max_capacity(10_000)
        .build()
});

static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build reqwest client")
});

/// DefiLlama coins API의 체인 키. 기본 "monad", env `DEFILLAMA_CHAIN`로 override.
static CHAIN: Lazy<String> =
    Lazy::new(|| std::env::var("DEFILLAMA_CHAIN").unwrap_or_else(|_| "monad".to_string()));

const BASE_URL: &str = "https://coins.llama.fi/prices/current";

/// 토큰 USD 가격 소스 — DefiLlama coins API. **컨트랙트 주소**로 batch 조회
/// (`{chain}:{addr},...`), 무키·무 rate-limit. 반환 맵 키는 **소문자 주소**.
pub struct DefiLlamaPriceSource {
    chain: String,
}

impl DefiLlamaPriceSource {
    pub fn new() -> Self {
        Self {
            chain: CHAIN.clone(),
        }
    }

    /// 실패(네트워크/비2xx/본문 읽기)는 `Err`로 반환 → 호출부가 캐시하지 않음.
    async fn fetch_batch(chain: &str, addrs: &[String]) -> anyhow::Result<String> {
        let keys: Vec<String> = addrs.iter().map(|a| format!("{chain}:{a}")).collect();
        let url = format!("{BASE_URL}/{}", keys.join(","));
        let body = HTTP
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(body)
    }
}

impl Default for DefiLlamaPriceSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceSource for DefiLlamaPriceSource {
    /// `ids` = 토큰 컨트랙트 주소들. 반환 맵 키 = 소문자 주소.
    async fn prices_usd(&self, ids: &[String]) -> HashMap<String, BigDecimal> {
        if ids.is_empty() {
            return HashMap::new();
        }
        // 소문자화 + dedup → URL/캐시 키 안정화(checksum 대소문자 무관). 응답 파싱·조회도 소문자 기준.
        let mut addrs: Vec<String> = ids.iter().map(|a| a.to_lowercase()).collect();
        addrs.sort();
        addrs.dedup();
        let cache_key = format!("llama:{}:{}", self.chain, addrs.join(","));
        let chain = self.chain.clone();
        // 성공만 캐시 — 일시적 실패는 Err로 전파되어 with_cache(try_get_with)가 저장하지 않음.
        let cached: anyhow::Result<String> = with_cache(&PRICE_CACHE, cache_key, || async {
            Self::fetch_batch(&chain, &addrs).await
        })
        .await;
        match cached {
            Ok(body) => parse_defillama_prices(&body),
            Err(_) => HashMap::new(),
        }
    }
}

/// `{ "coins": { "monad:0xAddr": { "price": 0.99, ... } } }` → 소문자 주소 → 가격.
/// 가격은 JSON 숫자 리터럴 그대로 파싱(f64 왕복 없이 정밀도 보존).
pub(crate) fn parse_defillama_prices(body: &str) -> HashMap<String, BigDecimal> {
    let mut out = HashMap::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return out;
    };
    let Some(coins) = json.get("coins").and_then(|c| c.as_object()) else {
        return out;
    };
    for (key, val) in coins {
        // key = "monad:0xAddr" → ':' 뒤가 주소
        let addr = key.split_once(':').map(|(_, a)| a).unwrap_or(key);
        let Some(price) = val
            .get("price")
            .filter(|p| p.is_number())
            .map(|p| p.to_string())
        else {
            continue;
        };
        if let Ok(bd) = BigDecimal::from_str(&price) {
            out.insert(addr.to_lowercase(), bd);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_lowercased_address_prices() {
        let body = r#"{"coins":{
            "monad:0x754704Bc059F8C67012fEd69BC8A327a5aafb603":{"decimals":6,"symbol":"USDC","price":0.9996326093575469,"confidence":0.99},
            "monad:0x0000000000000000000000000000000000000000":{"decimals":18,"symbol":"MON","price":0.021004619947610256,"confidence":0.99}
        }}"#;
        let m = parse_defillama_prices(body);
        assert_eq!(
            m.get("0x754704bc059f8c67012fed69bc8a327a5aafb603"),
            Some(&BigDecimal::from_str("0.9996326093575469").unwrap()),
            "USDC 가격을 소문자 주소 키로 추출"
        );
        assert_eq!(
            m.get("0x0000000000000000000000000000000000000000"),
            Some(&BigDecimal::from_str("0.021004619947610256").unwrap()),
            "native MON(0x0) 도 파싱"
        );
    }

    #[test]
    fn parse_empty_on_garbage_or_missing_coins() {
        assert!(parse_defillama_prices("not json").is_empty());
        assert!(parse_defillama_prices(r#"{"coins":{}}"#).is_empty());
        assert!(parse_defillama_prices(r#"{}"#).is_empty());
    }
}
