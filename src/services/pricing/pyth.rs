use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use moka::future::Cache;
use once_cell::sync::Lazy;

use crate::config::PYTH_HERMES_URL;
use crate::services::pricing::{PriceSource, normalize_feed_id, parse_hermes_prices};
use crate::utils::single_flight::with_cache;

/// feed_id → USD 가격 캐시 (10초 TTL). 키는 정규화된 feed_id.
static PRICE_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(10))
        .max_capacity(10_000)
        .build()
});

/// 공유 reqwest 클라이언트.
static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("failed to build reqwest client")
});

pub struct PythHermesClient;

impl PythHermesClient {
    pub fn new() -> Self {
        Self
    }

    /// 단일 feed_id의 USD 가격을 Hermes에서 조회(캐시 경유). 실패 시 None.
    async fn fetch_one(feed_id: &str) -> Option<BigDecimal> {
        let key = normalize_feed_id(feed_id);
        // String을 캐시 가능한 값으로: BigDecimal은 to_plain_string으로 직렬화.
        let cached: anyhow::Result<Option<String>> =
            with_cache(&PRICE_CACHE, format!("pyth:{key}"), || async {
                let url = format!(
                    "{}/v2/updates/price/latest?ids[]={}",
                    PYTH_HERMES_URL.as_str(),
                    key
                );
                let body = HTTP.get(&url).send().await?.text().await?;
                let price = parse_hermes_prices(&body).remove(&key);
                Ok(price.map(|p| p.to_plain_string()))
            })
            .await;
        cached
            .ok()
            .flatten()
            .and_then(|s| s.parse::<BigDecimal>().ok())
    }
}

impl Default for PythHermesClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceSource for PythHermesClient {
    async fn prices_usd(&self, feed_ids: &[String]) -> HashMap<String, BigDecimal> {
        let mut out = HashMap::new();
        for id in feed_ids {
            if let Some(price) = Self::fetch_one(id).await {
                out.insert(normalize_feed_id(id), price);
            }
        }
        out
    }
}
