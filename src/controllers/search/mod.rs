use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    types::{
        common::info::{AccountInfo, TokenInfo},
        search::{
            SearchAccount, SearchAccountResponse, SearchResponse, SearchToken, SearchTokenResponse,
        },
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct SearchController {
    db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn search(&self, query: &str) -> Result<SearchResponse> {
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

        let cache_key = cache_key!("search", query.trim().to_lowercase());

        let result = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_search_results(query).await
        })
        .await?;

        Ok(result)
    }

    async fn fetch_search_results(&self, query: &str) -> Result<SearchResponse> {
        let search_pattern = self.analyze_search_pattern(query);

        let search_start = std::time::Instant::now();
        let (token_result, account_result) = tokio::join!(
            async {
                let token_start = std::time::Instant::now();
                let result = self.search_tokens_by_pattern(query, &search_pattern).await;
                tracing::info!(
                    "Token search completed - query: {}, time: {:?}",
                    query,
                    token_start.elapsed()
                );
                result
            },
            async {
                let account_start = std::time::Instant::now();
                let result = self
                    .search_accounts_by_pattern(query, &search_pattern)
                    .await;
                tracing::info!(
                    "Account search completed - query: {}, time: {:?}",
                    query,
                    account_start.elapsed()
                );
                result
            }
        );
        tracing::info!(
            "Total search time - query: {}, time: {:?}",
            query,
            search_start.elapsed()
        );

        let token_records = token_result?;
        let account_records = account_result?;

        let tokens_vec = token_records
            .into_iter()
            .map(|row| SearchToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                market_cap: (row.total_supply.clone() * row.price.clone()).to_string(),
                price: row.price.to_plain_string(),
                created_at: row.created_at,
            })
            .collect::<Vec<_>>();

        let accounts_vec = account_records
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
            .collect::<Vec<_>>();

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

    fn analyze_search_pattern(&self, query: &str) -> SearchPattern {
        let trimmed_query = query.trim();

        if trimmed_query.starts_with('@') {
            return SearchPattern::TwitterHandle;
        }

        if trimmed_query.len() == 42 && trimmed_query.starts_with("0x") {
            return SearchPattern::EvmAddress;
        }

        SearchPattern::Universal
    }

    async fn search_tokens_by_pattern(
        &self,
        query: &str,
        pattern: &SearchPattern,
    ) -> Result<Vec<SearchTokenRow>> {
        let pool = self.db.get_read_pool();

        match pattern {
            SearchPattern::TwitterHandle => Ok(vec![]),
            SearchPattern::EvmAddress => sqlx::query_as::<_, SearchTokenRow>(
                r#"
                    SELECT t.token_id, t.name, t.symbol, t.image_uri,
                           t.created_at, t.total_supply, m.market_type, m.price
                    FROM token t
                    JOIN market m ON t.token_id = m.token_id
                    WHERE LOWER(t.token_id) = LOWER($1)
                    LIMIT 1
                    "#,
            )
            .bind(query)
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow::anyhow!("Database error: {}", e)),
            SearchPattern::Universal => {
                let query_clone = query.to_string();

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

                let (symbol_results, name_results) = tokio::join!(symbol_future, name_future);

                let mut combined_results = Vec::new();
                let mut seen_ids = HashSet::new();

                for token in symbol_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(token.token_id.clone()) {
                        combined_results.push(token);
                    }
                }

                for token in name_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(token.token_id.clone()) && combined_results.len() < 50 {
                        combined_results.push(token);
                    }
                }

                combined_results.sort_by(|a, b| b.price.cmp(&a.price));
                combined_results.truncate(50);
                Ok(combined_results)
            }
        }
    }

    async fn search_accounts_by_pattern(
        &self,
        query: &str,
        pattern: &SearchPattern,
    ) -> Result<Vec<SearchAccountRow>> {
        let pool = self.db.get_read_pool();

        match pattern {
            SearchPattern::TwitterHandle => {
                sqlx::query_as::<_, SearchAccountRow>(
                    r#"
                    SELECT
                        a.account_id,
                        a.nickname,
                        a.image_uri,
                        a.follower_count,
                        a.following_count,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        COALESCE(total_value.value, 0) as total_value
                    FROM account_x ax
                    JOIN account a ON ax.account_id = a.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    LEFT JOIN LATERAL (
                        SELECT SUM(b.balance * m.price) as value
                        FROM balance b
                        INNER JOIN market m ON b.token_id = m.token_id
                        WHERE b.account_id = a.account_id
                          AND b.balance >= 1000000000000000000
                    ) total_value ON true
                    WHERE ax.x_handle ILIKE '%' || $1 || '%'
                    ORDER BY total_value.value DESC NULLS LAST
                    LIMIT 50
                    "#,
                )
                .bind(query)
                .fetch_all(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))
            }
            SearchPattern::EvmAddress => {
                sqlx::query_as::<_, SearchAccountRow>(
                    r#"
                    SELECT
                        a.account_id,
                        a.nickname,
                        a.image_uri,
                        a.follower_count,
                        a.following_count,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        COALESCE(total_value.value, 0) as total_value
                    FROM account a
                    LEFT JOIN LATERAL (
                        SELECT x_handle, x_image_uri, is_blue_label
                        FROM account_x
                        WHERE account_id = a.account_id
                        LIMIT 1
                    ) ax ON true
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    LEFT JOIN LATERAL (
                        SELECT SUM(b.balance * m.price) as value
                        FROM balance b
                        INNER JOIN market m ON b.token_id = m.token_id
                        WHERE b.account_id = a.account_id
                          AND b.balance >= 1000000000000000000
                    ) total_value ON true
                    WHERE LOWER(a.account_id) = LOWER($1)
                    LIMIT 1
                    "#,
                )
                .bind(query)
                .fetch_all(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Database error: {}", e))
            }
            SearchPattern::Universal => {
                let (nickname_future, x_handle_future) = (
                    sqlx::query_as::<_, SearchAccountRow>(
                        r#"
                        SELECT
                            a.account_id,
                            a.nickname,
                            a.image_uri,
                            a.follower_count,
                            a.following_count,
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                            ax.x_image_uri,
                            ax.is_blue_label,
                            COALESCE(total_value.value, 0) as total_value
                        FROM account a
                        LEFT JOIN LATERAL (
                            SELECT x_handle, x_image_uri, is_blue_label
                            FROM account_x
                            WHERE account_id = a.account_id
                            LIMIT 1
                        ) ax ON true
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                        LEFT JOIN LATERAL (
                            SELECT SUM(b.balance * m.price) as value
                            FROM balance b
                            INNER JOIN market m ON b.token_id = m.token_id
                            WHERE b.account_id = a.account_id
                              AND b.balance >= 1000000000000000000
                        ) total_value ON true
                        WHERE a.nickname ILIKE '%' || $1 || '%'
                        ORDER BY total_value.value DESC NULLS LAST, a.follower_count DESC
                        LIMIT 5
                        "#,
                    )
                    .bind(query)
                    .fetch_all(pool),
                    sqlx::query_as::<_, SearchAccountRow>(
                        r#"
                        SELECT
                            a.account_id,
                            a.nickname,
                            a.image_uri,
                            a.follower_count,
                            a.following_count,
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                            ax.x_image_uri,
                            ax.is_blue_label,
                            COALESCE(total_value.value, 0) as total_value
                        FROM account_x ax
                        JOIN account a ON ax.account_id = a.account_id
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                        LEFT JOIN LATERAL (
                            SELECT SUM(b.balance * m.price) as value
                            FROM balance b
                            INNER JOIN market m ON b.token_id = m.token_id
                            WHERE b.account_id = a.account_id
                              AND b.balance >= 1000000000000000000
                        ) total_value ON true
                        WHERE ax.x_handle ILIKE '%' || $1 || '%'
                        ORDER BY total_value.value DESC NULLS LAST
                        LIMIT 20
                        "#,
                    )
                    .bind(query)
                    .fetch_all(pool),
                );

                let (nickname_results, x_handle_results) = tokio::join!(nickname_future, x_handle_future);

                let mut combined_results = Vec::new();
                let mut seen_ids = HashSet::new();

                for account in nickname_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(account.account_id.clone()) {
                        combined_results.push(account);
                    }
                }

                for account in x_handle_results.map_err(|e| anyhow::anyhow!("Database error: {}", e))? {
                    if seen_ids.insert(account.account_id.clone())
                        && combined_results.len() < 40
                    {
                        combined_results.push(account);
                    }
                }

                combined_results.sort_by(|a, b| b.total_value.cmp(&a.total_value));
                combined_results.truncate(40);
                Ok(combined_results)
            }
        }
    }
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

enum SearchPattern {
    EvmAddress,
    TwitterHandle,
    Universal,
}
