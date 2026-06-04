use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use once_cell::sync::Lazy;
use tracing::error;

use crate::{
    controllers::dex::{
        pool::PoolController, position::PositionController, reserves::ReservesController,
        tokens::TokensController,
    },
    db::postgres::PostgresDatabase,
    result::AppError,
    types::dex::{
        pool::PoolDetailResponse,
        position::LpPositionsResponse,
        reserves::ReservesResponse,
        tokens::{DexTokenListQuery, DexTokenListResponse},
    },
    utils::single_flight::with_cache,
};

/// `/dex/tokens` 응답 캐시 (moka + single-flight).
/// - 계정 없는 목록: 계정 무관(가격/메타만) → 30초.
/// - 계정 있는 목록: 잔액(RPC)·balance_usd(Pyth) 포함 → 10초.
/// 가격 소스(Pyth)·balanceOf 자체 캐시는 별도로 항상 10초.
static DEX_TOKENS_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(30))
        .max_capacity(10_000)
        .build()
});
static DEX_TOKENS_ACCOUNT_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(10))
        .max_capacity(50_000)
        .build()
});

/// `/dex/reserves` 캐시. PK 단건이라 가볍지만, 핫풀 동시 폴링(thundering herd)을
/// single-flight로 1회 DB 조회로 합친다. reserve 신선도 우선 → **1초**.
static DEX_RESERVES_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(1))
        .max_capacity(50_000)
        .build()
});
/// `/dex/positions/{account_id}` 캐시. 계정별 LP 포지션(whitelist RPC 잔액 보강 포함,
/// 비교적 무거움) → **5초**. 키에 account_id 포함 → 계정 간 공유 없음.
static DEX_POSITIONS_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(5))
        .max_capacity(50_000)
        .build()
});
/// `/dex/pools/{pool_id}` 캐시. reserve/TVL/APR. reserve 신선도 우선 → **1초**.
static DEX_POOL_CACHE: Lazy<Cache<String, Arc<Vec<u8>>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(1))
        .max_capacity(50_000)
        .build()
});

/// 캐시 키. 계정(핸들러에서 EIP-55 검증·정규화된 값)을 첫 세그먼트에 넣어 **계정 간 절대
/// 공유되지 않게** 한다(잔액 유출 방지). 자유 텍스트 `q`는 **맨 뒤**에 둬서 `:` 포함 입력이
/// 앞쪽 구조 세그먼트(account/page/limit)를 밀어 충돌시키는 것을 원천 차단한다.
fn dex_tokens_cache_key(query: &DexTokenListQuery) -> String {
    format!(
        "dextok:{}:{}:{}:{}",
        query.account.as_deref().unwrap_or("-"),
        query.page,
        query.limit,
        query.q.as_deref().unwrap_or("-"),
    )
}

pub struct DexService {
    postgres: Arc<PostgresDatabase>,
}

impl DexService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_positions(&self, account_id: &str) -> Result<LpPositionsResponse, AppError> {
        let key = format!("dexpos:{}", account_id);
        let postgres = self.postgres.clone();
        let account = account_id.to_string();
        with_cache(&DEX_POSITIONS_CACHE, key, move || async move {
            PositionController::new(postgres).get_positions(&account).await
        })
        .await
        .map_err(|err| {
            error!(
                "Failed to get LP positions: account_id={}, error={}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })
    }

    pub async fn get_pool_detail(
        &self,
        pool_id: &str,
    ) -> Result<Option<PoolDetailResponse>, AppError> {
        let key = format!("dexpool:{}", pool_id);
        let postgres = self.postgres.clone();
        let pid = pool_id.to_string();
        with_cache(&DEX_POOL_CACHE, key, move || async move {
            PoolController::new(postgres).get_pool_detail(&pid).await
        })
        .await
        .map_err(|err| {
            error!(
                "Failed to get pool detail: pool_id={}, error={}",
                pool_id, err
            );
            AppError::InternalError(err.to_string())
        })
    }

    pub async fn get_reserves(
        &self,
        pool_id: &str,
    ) -> Result<Option<ReservesResponse>, AppError> {
        let key = format!("dexres:{}", pool_id);
        let postgres = self.postgres.clone();
        let pid = pool_id.to_string();
        with_cache(&DEX_RESERVES_CACHE, key, move || async move {
            ReservesController::new(postgres).get_reserves(&pid).await
        })
        .await
        .map_err(|err| {
            error!("Failed to get reserves: pool_id={}, error={}", pool_id, err);
            AppError::InternalError(err.to_string())
        })
    }

    pub async fn get_tokens(
        &self,
        query: &DexTokenListQuery,
    ) -> Result<DexTokenListResponse, AppError> {
        let cache = if query.account.is_some() {
            &*DEX_TOKENS_ACCOUNT_CACHE
        } else {
            &*DEX_TOKENS_CACHE
        };
        let key = dex_tokens_cache_key(query);
        let postgres = self.postgres.clone();
        let q = query.clone();
        with_cache(cache, key, move || async move {
            TokensController::new(postgres).list_tokens(&q).await
        })
        .await
        .map_err(|err| {
            error!("Failed to list dex tokens: error={}", err);
            AppError::InternalError(err.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::dex_tokens_cache_key;
    use crate::types::dex::tokens::DexTokenListQuery;

    fn q(account: Option<&str>, query: Option<&str>, page: i64, limit: i64) -> DexTokenListQuery {
        DexTokenListQuery {
            account: account.map(String::from),
            q: query.map(String::from),
            page,
            limit,
        }
    }

    #[test]
    fn cache_key_isolates_accounts() {
        // 서로 다른 계정은 절대 같은 키를 공유하면 안 된다 (잔액 유출 방지).
        let a = dex_tokens_cache_key(&q(Some("0xAAA"), None, 1, 50));
        let b = dex_tokens_cache_key(&q(Some("0xBBB"), None, 1, 50));
        assert_ne!(a, b);
        // 계정 없음 vs 있음도 분리.
        assert_ne!(dex_tokens_cache_key(&q(None, None, 1, 50)), a);
    }

    #[test]
    fn cache_key_varies_by_query_and_pagination() {
        let base = dex_tokens_cache_key(&q(None, None, 1, 50));
        assert_ne!(base, dex_tokens_cache_key(&q(None, Some("usdc"), 1, 50)));
        assert_ne!(base, dex_tokens_cache_key(&q(None, None, 2, 50)));
        assert_ne!(base, dex_tokens_cache_key(&q(None, None, 1, 20)));
    }

    #[test]
    fn cache_key_stable_for_same_query() {
        assert_eq!(
            dex_tokens_cache_key(&q(Some("0xAAA"), Some("u"), 1, 50)),
            dex_tokens_cache_key(&q(Some("0xAAA"), Some("u"), 1, 50)),
        );
    }
}
