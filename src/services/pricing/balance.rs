use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use moka::future::Cache;
use once_cell::sync::Lazy;
use tracing::warn;

use crate::config::RPC_URL;
use crate::services::pricing::BalanceSource;
use crate::utils::single_flight::with_cache;

/// (token_id, account) → raw wei 잔액 문자열 캐시 (10초 TTL).
static BALANCE_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(10))
        .max_capacity(50_000)
        .build()
});

pub struct RpcBalanceSource;

impl RpcBalanceSource {
    pub fn new() -> Self {
        Self
    }

    async fn fetch(token_id: &str, account: &str) -> anyhow::Result<BigDecimal> {
        use alloy::primitives::{Address, U256};
        use alloy::providers::ProviderBuilder;
        use alloy::sol;

        sol! {
            #[allow(missing_docs)]
            #[sol(rpc)]
            interface IERC20 {
                function balanceOf(address account) external view returns (uint256);
            }
        }

        let rpc_url: url::Url = RPC_URL.parse()?;
        let provider = ProviderBuilder::new().connect_http(rpc_url);
        let token_addr: Address = token_id.parse()?;
        let account_addr: Address = account.parse()?;
        let contract = IERC20::new(token_addr, provider);
        let bal: U256 = contract.balanceOf(account_addr).call().await?;
        Ok(BigDecimal::from_str(&bal.to_string())?)
    }
}

impl Default for RpcBalanceSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BalanceSource for RpcBalanceSource {
    async fn balance_of(&self, token_id: &str, account: &str) -> Option<BigDecimal> {
        let key = format!("bal:{}:{}", token_id, account);
        let cached: anyhow::Result<Option<String>> = with_cache(&BALANCE_CACHE, key, || async {
            match Self::fetch(token_id, account).await {
                Ok(b) => Ok(Some(b.to_plain_string())),
                Err(e) => {
                    warn!("balanceOf failed token={token_id} account={account}: {e}");
                    Ok(None) // degrade: 실패를 캐시하되 null로
                }
            }
        })
        .await;
        cached
            .ok()
            .flatten()
            .and_then(|s| s.parse::<BigDecimal>().ok())
    }
}
