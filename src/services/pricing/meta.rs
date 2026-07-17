use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use once_cell::sync::Lazy;
use tracing::warn;

use crate::config::RPC_URL;
use crate::services::pricing::{TokenMeta, TokenMetaSource};
use crate::utils::single_flight::with_cache;

/// token_id → ERC20 메타 캐시. 메타는 사실상 불변이라 길게(1h) 캐시한다.
/// 실패(비표준 컨트랙트/EOA 등)도 None으로 캐시해 반복 RPC 폭주를 막는다.
static META_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(3600))
        .max_capacity(50_000)
        .build()
});

pub struct RpcMetaSource;

impl RpcMetaSource {
    pub fn new() -> Self {
        Self
    }

    async fn fetch(token_id: &str) -> anyhow::Result<TokenMeta> {
        use alloy::primitives::Address;
        use alloy::providers::ProviderBuilder;
        use alloy::sol;

        sol! {
            #[allow(missing_docs)]
            #[sol(rpc)]
            interface IERC20Meta {
                function name() external view returns (string);
                function symbol() external view returns (string);
                function decimals() external view returns (uint8);
            }
        }

        let rpc_url: url::Url = RPC_URL.parse()?;
        let provider = ProviderBuilder::new().connect_http(rpc_url);
        let addr: Address = token_id.parse()?;
        let contract = IERC20Meta::new(addr, provider);

        let name: String = contract.name().call().await?;
        let symbol: String = contract.symbol().call().await?;
        let decimals: u8 = contract.decimals().call().await?;

        Ok(TokenMeta {
            name,
            symbol,
            decimals: decimals as i32,
        })
    }
}

impl Default for RpcMetaSource {
    fn default() -> Self {
        Self::new()
    }
}

/// token_id → on-chain `IERC20.totalSupply()` (raw, ×10^decimals) 캐시.
/// 공급량은 burn/mint로 변할 수 있어 메타(1h)보다 짧게 캐시한다.
/// 실패도 None으로 캐시해 반복 RPC 폭주를 막는다.
static SUPPLY_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(600))
        .max_capacity(50_000)
        .build()
});

impl RpcMetaSource {
    async fn fetch_total_supply(token_id: &str) -> anyhow::Result<String> {
        use alloy::primitives::Address;
        use alloy::providers::ProviderBuilder;
        use alloy::sol;

        sol! {
            #[allow(missing_docs)]
            #[sol(rpc)]
            interface IERC20Supply {
                function totalSupply() external view returns (uint256);
            }
        }

        let rpc_url: url::Url = RPC_URL.parse()?;
        let provider = ProviderBuilder::new().connect_http(rpc_url);
        let addr: Address = token_id.parse()?;
        let contract = IERC20Supply::new(addr, provider);

        let supply = contract.totalSupply().call().await?;
        Ok(supply.to_string())
    }

    /// on-chain `IERC20.totalSupply()`를 raw(×10^decimals) 십진 문자열로 반환.
    /// 10분 캐시, 실패(비표준 컨트랙트/EOA/노드 장애)는 None으로 캐시.
    pub async fn total_supply(token_id: &str) -> Option<String> {
        let key = format!("supply:{token_id}");
        let cached: anyhow::Result<Option<String>> = with_cache(&SUPPLY_CACHE, key, || async {
            match Self::fetch_total_supply(token_id).await {
                Ok(supply) => Ok(Some(supply)),
                Err(e) => {
                    warn!("total_supply RPC failed token={token_id}: {e}");
                    Ok(None) // degrade: 실패를 None으로 캐시
                }
            }
        })
        .await;
        cached.ok().flatten()
    }
}

#[async_trait]
impl TokenMetaSource for RpcMetaSource {
    async fn token_meta(&self, token_id: &str) -> Option<TokenMeta> {
        let key = format!("meta:{token_id}");
        let cached: anyhow::Result<Option<TokenMeta>> = with_cache(&META_CACHE, key, || async {
            match Self::fetch(token_id).await {
                Ok(m) => Ok(Some(m)),
                Err(e) => {
                    warn!("token_meta RPC failed token={token_id}: {e}");
                    Ok(None) // degrade: 실패를 None으로 캐시
                }
            }
        })
        .await;
        cached.ok().flatten()
    }
}
