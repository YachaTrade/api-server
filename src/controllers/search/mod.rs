use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    types::{
        common::info::{AccountInfo, MarketInfo, MarketType, TokenInfo},
        search::{
            AccountSearchResponse, AccountSearchResult, SearchResponse, TokenSearchResponse,
            TokenSearchResult,
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
                token_result: TokenSearchResponse {
                    total_count: 0,
                    tokens: vec![],
                },
                account_result: AccountSearchResponse {
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
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if row.market_type == "CURVE" && market_id.is_empty() {
                    market_id = BONDING_CURVE.clone();
                }

                TokenSearchResult {
                    token_info: TokenInfo {
                        token_id: row.token_id.clone(),
                        name: row.name,
                        symbol: row.symbol,
                        image_uri: row.image_uri,
                        description: row.description,
                        is_graduated: row.is_graduated,
                        is_nsfw: row.is_nsfw,
                        twitter: row.twitter,
                        telegram: row.telegram,
                        website: row.website,
                        created_at: row.created_at,
                        creator: AccountInfo {
                            account_id: row.creator,
                            nickname: row.creator_nickname,
                            bio: row.creator_bio,
                            image_uri: row.creator_image_uri,
                        },
                    },
                    market_info: MarketInfo {
                        market_type: match row.market_type.as_str() {
                            "CURVE" => MarketType::Curve,
                            "DEX" => MarketType::Dex,
                            _ => MarketType::Curve,
                        },
                        token_id: row.token_id,
                        market_id,
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        price: row.price.normalized().to_plain_string(),
                        total_supply: row.total_supply.normalized().to_plain_string(),
                        liquidity: row.liquidity.normalized().to_plain_string(),
                        volume: row.volume.normalized().to_plain_string(),
                        ath_price: row.ath_price.normalized().to_plain_string(),
                        holder_count: row.holder_count,
                    },
                }
            })
            .collect::<Vec<_>>();

        let accounts_vec = account_records
            .into_iter()
            .map(|row| AccountSearchResult {
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    bio: row.bio,
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                },
                total_value: row.total_value.to_string(),
            })
            .collect::<Vec<_>>();

        Ok(SearchResponse {
            token_result: TokenSearchResponse {
                total_count: tokens_vec.len() as i64,
                tokens: tokens_vec,
            },
            account_result: AccountSearchResponse {
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
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        t.total_supply,
                        COALESCE(m.reserve_native, 0) as liquidity,
                        m.volume,
                        m.ath_price
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON t.token_id = m.token_id
                    CROSS JOIN latest_price lp
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
                        WITH latest_price AS (
                            SELECT price
                            FROM price
                            ORDER BY created_at DESC
                            LIMIT 1
                        )
                        SELECT
                            t.token_id,
                            t.name,
                            t.symbol,
                            t.image_uri,
                            t.description,
                            t.twitter,
                            t.telegram,
                            t.website,
                            t.is_graduated,
                            t.is_nsfw,
                            t.created_at,
                            t.creator,
                            t.token_holder_count as holder_count,
                            COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                            a.bio as creator_bio,
                            COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                            m.market_type,
                            COALESCE(m.pool_id, '') as market_id,
                            (m.price * COALESCE(lp.price, 0)) as token_price,
                            COALESCE(lp.price, 0) as native_price,
                            m.price,
                            t.total_supply,
                            COALESCE(m.reserve_native, 0) as liquidity,
                            m.volume,
                            m.ath_price
                        FROM token t
                        JOIN account a ON t.creator = a.account_id
                        LEFT JOIN account_x ax ON a.account_id = ax.account_id
                        JOIN market m ON t.token_id = m.token_id
                        CROSS JOIN latest_price lp
                        WHERE t.symbol ILIKE '%' || $1 || '%'
                        ORDER BY (m.price * t.total_supply * COALESCE(lp.price, 0)) DESC, t.symbol DESC
                        LIMIT 25
                        "#,
                    )
                    .bind(&query)
                    .fetch_all(pool),
                    sqlx::query_as::<_, SearchTokenRow>(
                        r#"
                        WITH latest_price AS (
                            SELECT price
                            FROM price
                            ORDER BY created_at DESC
                            LIMIT 1
                        )
                        SELECT
                            t.token_id,
                            t.name,
                            t.symbol,
                            t.image_uri,
                            t.description,
                            t.twitter,
                            t.telegram,
                            t.website,
                            t.is_graduated,
                            t.is_nsfw,
                            t.created_at,
                            t.creator,
                            t.token_holder_count as holder_count,
                            COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                            a.bio as creator_bio,
                            COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                            m.market_type,
                            COALESCE(m.pool_id, '') as market_id,
                            (m.price * COALESCE(lp.price, 0)) as token_price,
                            COALESCE(lp.price, 0) as native_price,
                            m.price,
                            t.total_supply,
                            COALESCE(m.reserve_native, 0) as liquidity,
                            m.volume,
                            m.ath_price
                        FROM token t
                        JOIN account a ON t.creator = a.account_id
                        LEFT JOIN account_x ax ON a.account_id = ax.account_id
                        JOIN market m ON t.token_id = m.token_id
                        CROSS JOIN latest_price lp
                        WHERE t.name ILIKE '%' || $1 || '%'
                        ORDER BY (m.price * t.total_supply * COALESCE(lp.price, 0)) DESC, t.name DESC
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

                combined_results.sort_by(|a, b| b.token_price.cmp(&a.token_price));
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
                        a.bio,
                        a.image_uri,
                        ax.x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        COALESCE(total_value.value, 0) as total_value
                    FROM account_x ax
                    JOIN account a ON ax.account_id = a.account_id
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
                        a.bio,
                        a.image_uri,
                        ax.x_handle,
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
                            a.bio,
                            a.image_uri,
                            ax.x_handle,
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
                            a.bio,
                            a.image_uri,
                            ax.x_handle,
                            ax.x_image_uri,
                            ax.is_blue_label,
                            COALESCE(total_value.value, 0) as total_value
                        FROM account_x ax
                        JOIN account a ON ax.account_id = a.account_id
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
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_graduated: bool,
    is_nsfw: bool,
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: String,
    market_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    total_supply: BigDecimal,
    liquidity: BigDecimal,
    volume: BigDecimal,
    ath_price: BigDecimal,
}

#[derive(sqlx::FromRow)]
struct SearchAccountRow {
    account_id: String,
    nickname: String,
    bio: String,
    image_uri: String,
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
