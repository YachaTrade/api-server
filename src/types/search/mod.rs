use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

use super::common::info::{AccountInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchToken {
    pub token_info: TokenInfo,
    pub price: BigDecimal,
    pub market_cap: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchTokenResponse {
    pub tokens: Vec<SearchToken>,
    pub total_count: i64,
}

#[derive(sqlx::FromRow)]
struct SearchTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    image_uri: String,
    created_at: i64,
    total_supply: BigDecimal,
    market_type: String,
    price: BigDecimal,
}

#[derive(sqlx::FromRow)]
struct SearchAccountRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
    total_value: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccount {
    pub account_info: AccountInfo,
    pub total_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccountResponse {
    pub accounts: Vec<SearchAccount>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub tokens: SearchTokenResponse,
    pub accounts: SearchAccountResponse,
}

pub struct SearchController {
    pub db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn search(&self, query: &str) -> Result<SearchResponse> {
        let start_time = Instant::now();

        if query.trim().is_empty() {
            return Ok(SearchResponse {
                tokens: SearchTokenResponse {
                    total_count: 0,
                    tokens: vec![],
                },
                accounts: SearchAccountResponse {
                    total_count: 0,
                    accounts: vec![],
                },
            });
        }

        // 캐시 키 생성
        let cache_key = cache_key!("search", query.trim().to_lowercase());

        // Single Flight Pattern 적용
        let result = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_search_results(query).await
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!("search(query: {}) completed in {:?}", query, elapsed);
        Ok(result)
    }

    async fn fetch_search_results(&self, query: &str) -> Result<SearchResponse> {
        // 검색 패턴 분석
        let search_pattern = self.analyze_search_pattern(query);

        // 토큰 검색과 계정 검색 시간 분리 측정
        let search_start = std::time::Instant::now();
        let (token_result, account_result) = tokio::join!(
            async {
                let token_start = std::time::Instant::now();
                let result = self.search_tokens_by_pattern(query, &search_pattern).await;
                tracing::info!("Token search completed - query: {}, time: {:?}", query, token_start.elapsed());
                result
            },
            async {
                let account_start = std::time::Instant::now(); 
                let result = self.search_accounts_by_pattern(query, &search_pattern).await;
                tracing::info!("Account search completed - query: {}, time: {:?}", query, account_start.elapsed());
                result
            }
        );
        tracing::info!("Total search time - query: {}, time: {:?}", query, search_start.elapsed());

        let token_records = token_result?;
        let account_records = account_result?;

        let tokens_vec: Vec<SearchToken> = token_records
            .into_iter()
            .map(|row| SearchToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                market_cap: (row.total_supply.clone() * row.price.clone()).to_string(),

                price: row.price,
                created_at: row.created_at,
            })
            .collect();

        let accounts_vec: Vec<SearchAccount> = account_records
            .into_iter()
            .map(|row| SearchAccount {
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                },
                total_value: row.total_value.to_string(),
            })
            .collect();

        Ok(SearchResponse {
            tokens: SearchTokenResponse {
                total_count: tokens_vec.len() as i64,
                tokens: tokens_vec,
            },
            accounts: SearchAccountResponse {
                total_count: accounts_vec.len() as i64,
                accounts: accounts_vec,
            },
        })
    }

    // 검색 패턴 분석
    fn analyze_search_pattern(&self, query: &str) -> SearchPattern {
        let trimmed_query = query.trim();

        // @ 한들 패턴 (account_x에서 검색)
        if trimmed_query.starts_with('@') {
            return SearchPattern::TwitterHandle;
        }

        // EVM 주소 패턴 (42자 0x로 시작)
        if trimmed_query.len() == 42 && trimmed_query.starts_with("0x") {
            return SearchPattern::EvmAddress;
        }

        // 나머지는 모두 Trigram 검색
        SearchPattern::Universal
    }

    // 토큰 검색 (패턴별)
    async fn search_tokens_by_pattern(
        &self,
        query: &str,
        pattern: &SearchPattern,
    ) -> Result<Vec<SearchTokenRow>> {
        let pool = self.db.get_read_pool();

        match pattern {
            SearchPattern::TwitterHandle => {
                // @ 한들은 토큰 검색에서 제외
                Ok(vec![])
            }
            SearchPattern::EvmAddress => {
                // EVM 주소: 토큰 ID로 직접 검색 (Primary Key 접근)
                sqlx::query_as::<_, SearchTokenRow>(
                    r#"
                    SELECT t.token_id, t.name, t.symbol, t.image_uri,
                           t.created_at, t.total_supply, m.market_type, m.price
                    FROM token t
                    JOIN market m ON t.token_id = m.token_id
                    WHERE t.token_id ILIKE $1
                    LIMIT 1
                    "#,
                )
                .bind(query)
                .fetch_all(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))
            }
            SearchPattern::Universal => {
                // 병렬 검색 전략: symbol과 name을 동시에 검색
                let query_clone = query.to_string();

                // symbol과 name을 병렬로 검색
                let (symbol_future, name_future) = (
                    sqlx::query_as::<_, SearchTokenRow>(
                        r#"
                        SELECT t.token_id, t.name, t.symbol, t.image_uri,
                               t.created_at, t.total_supply, m.market_type, m.price
                        FROM token t
                        JOIN market m ON t.token_id = m.token_id
                        WHERE t.symbol ILIKE '%' || $1 || '%'
                        ORDER BY m.price DESC, t.symbol DESC
                        LIMIT 25
                        "#,
                    )
                    .bind(&query)
                    .fetch_all(pool),
                    sqlx::query_as::<_, SearchTokenRow>(
                        r#"
                        SELECT t.token_id, t.name, t.symbol, t.image_uri,
                               t.created_at, t.total_supply, m.market_type, m.price
                        FROM token t
                        JOIN market m ON t.token_id = m.token_id
                        WHERE t.name ILIKE '%' || $1 || '%'
                        ORDER BY m.price DESC, t.name DESC
                        LIMIT 25
                        "#,
                    )
                    .bind(&query_clone)
                    .fetch_all(pool),
                );

                // 두 결과를 동시에 기다림
                let (symbol_results, name_results) = tokio::join!(symbol_future, name_future);

                let mut combined_results = Vec::new();
                let mut seen_ids = std::collections::HashSet::new();

                // symbol 결과 추가 (중복 제거)
                for token in symbol_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(token.token_id.clone()) {
                        combined_results.push(token);
                    }
                }

                // name 결과 추가 (중복 제거)
                for token in name_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(token.token_id.clone()) && combined_results.len() < 50 {
                        combined_results.push(token);
                    }
                }

                // price 순으로 정렬 (높은 가격 우선)
                combined_results.sort_by(|a, b| b.price.cmp(&a.price));
                
                // 최대 50개로 제한
                combined_results.truncate(50);
                Ok(combined_results)
            }
        }
    }

    // 계정 검색 (패턴별)
    async fn search_accounts_by_pattern(
        &self,
        query: &str,
        pattern: &SearchPattern,
    ) -> Result<Vec<SearchAccountRow>> {
        let pool = self.db.get_read_pool();

        match pattern {
            SearchPattern::TwitterHandle => {
                // @ 시작하는 건 x_handle만 검색
                sqlx::query_as::<_, SearchAccountRow>(
                    r#"
                    SELECT a.account_id, a.nickname, a.image_uri,
                           a.follower_count, a.following_count,
                           CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                           ax.x_image_uri, ax.is_blue_label,
                           COALESCE((
                               SELECT SUM(b.balance * m.price)
                               FROM balance b
                               JOIN market m ON b.token_id = m.token_id
                               WHERE b.account_id = a.account_id 
                               AND b.balance >= 1000000000000000000
                           ), 0) as total_value
                    FROM account_x ax
                    JOIN account a ON ax.account_id = a.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    WHERE ax.x_handle ILIKE '%' || $1 || '%'
                    ORDER BY total_value DESC
                    LIMIT 50
                    "#,
                )
                .bind(query)
                .fetch_all(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))
            }
            SearchPattern::EvmAddress => {
                // EVM 주소: 계정 ID로 직접 검색 (Primary Key 접근)
                sqlx::query_as::<_, SearchAccountRow>(
                    r#"
                    SELECT a.account_id, a.nickname, a.image_uri,
                           a.follower_count, a.following_count,
                           CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                           ax.x_image_uri, ax.is_blue_label,
                           COALESCE((
                               SELECT SUM(b.balance * m.price)
                               FROM balance b
                               JOIN market m ON b.token_id = m.token_id
                               WHERE b.account_id = a.account_id 
                               AND b.balance >= 1000000000000000000
                           ), 0) as total_value
                    FROM account a
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    WHERE a.account_id ILIKE $1
                    LIMIT 1
                    "#,
                )
                .bind(query)
                .fetch_all(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))
            }
            SearchPattern::Universal => {
                // nickname과 x_handle 각각 별도로 검색하여 결과 합치기
                let (nickname_future, x_handle_future) = (
                    sqlx::query_as::<_, SearchAccountRow>(
                        r#"
                        WITH limited_accounts AS (
                            SELECT a.account_id, a.nickname, a.image_uri,
                                   a.follower_count, a.following_count
                            FROM account a
                            WHERE a.nickname ILIKE '%' || $1 || '%'
                            ORDER BY follower_count DESC
                            LIMIT 20
                        )
                        SELECT la.account_id, la.nickname, la.image_uri,
                               la.follower_count, la.following_count,
                               CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                               ax.x_image_uri, ax.is_blue_label,
                               COALESCE((
                                   SELECT SUM(b.balance * m.price)
                                   FROM balance b
                                   JOIN market m ON b.token_id = m.token_id
                                   WHERE b.account_id = la.account_id 
                                   AND b.balance >= 1000000000000000000
                               ), 0) as total_value
                        FROM limited_accounts la
                        LEFT JOIN account_x ax ON la.account_id = ax.account_id
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                        ORDER BY total_value DESC
                        LIMIT 20
                        "#,
                    )
                    .bind(query)
                    .fetch_all(pool),
                    sqlx::query_as::<_, SearchAccountRow>(
                        r#"
                        SELECT a.account_id, a.nickname, a.image_uri,
                               a.follower_count, a.following_count,
                               CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                               ax.x_image_uri, ax.is_blue_label,
                               COALESCE((
                                   SELECT SUM(b.balance * m.price)
                                   FROM balance b
                                   JOIN market m ON b.token_id = m.token_id
                                   WHERE b.account_id = a.account_id 
                                   AND b.balance >= 1000000000000000000
                               ), 0) as total_value
                        FROM account_x ax
                        JOIN account a ON ax.account_id = a.account_id
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                        WHERE ax.x_handle ILIKE '%' || $1 || '%'
                        ORDER BY total_value DESC
                        LIMIT 20
                        "#,
                    )
                    .bind(query)
                    .fetch_all(pool),
                );

                // 두 결과를 동시에 기다림
                let (nickname_results, x_handle_results) = tokio::join!(nickname_future, x_handle_future);

                let mut combined_results = Vec::new();
                let mut seen_ids = HashSet::new();

                // nickname 결과 추가 (중복 제거)
                for account in nickname_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(account.account_id.clone()) {
                        combined_results.push(account);
                    }
                }

                // x_handle 결과 추가 (중복 제거)
                for account in x_handle_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(account.account_id.clone()) && combined_results.len() < 50 {
                        combined_results.push(account);
                    }
                }

                combined_results.sort_by(|a, b| b.total_value.cmp(&a.total_value));

                // 최대 50개로 제한
                combined_results.truncate(50);
                Ok(combined_results)
            }
        }
    }
}

// 검색 패턴 enum
#[derive(Debug, Clone)]
enum SearchPattern {
    EvmAddress,    // 42자 0x로 시작하는 EVM 주소 (Primary Key 직접 접근)
    TwitterHandle, // @로 시작하는 트위터 한들 (account_x 테이블에서 검색)
    Universal,     // 일반 문자열 (Trigram 검색)
}
