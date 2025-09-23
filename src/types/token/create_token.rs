use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    types::common::{CountRow, info::TokenInfo, pagination::PaginationParams},
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};
use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedResponse {
    pub tokens: Vec<TokenCreated>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreated {
    pub token: TokenInfo,
    pub is_listing: bool,
    pub created_at: i64,
    pub market_cap: String,     //market cap -> price * total_supply
    pub total_supply: String,   // token table total_supply
    pub price: String,          // market table price
    pub current_amount: String, //position table current_token_amount
    pub description: Option<String>,
}

pub struct TokenCreatedController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenCreatedController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenCreatedController { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = cache_key!("token_created_count", account_id);

        // Single Flight Pattern 적용
        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = TokenCreatedController::new(db);
                controller.fetch_total_count(&account_id).await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_total_count(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(count)
    }

    async fn fetch_total_count(&self, account_id: &str) -> Result<i64> {
        let count = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM token t
                WHERE t.creator = $1
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        Ok(count.count)
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenCreatedResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = cache_key!(
            "tokens_created",
            account_id,
            pagination.page,
            pagination.limit
        );

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination;
            async move {
                let controller = TokenCreatedController::new(db);
                controller
                    .fetch_tokens_created(&account_id, &pagination)
                    .await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_tokens_created(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(response)
    }

    async fn fetch_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenCreatedResponse> {
        // Query tokens created by the account with their market and position information
        let offset = (pagination.page - 1) * pagination.limit;
        #[derive(sqlx::FromRow)]
        struct TokenCreatedRow {
            token_id: String,
            symbol: String,
            image_uri: String,
            name: String,
            is_listing: bool,
            created_at: i64,
            price: BigDecimal,
            total_supply: BigDecimal,
            market_cap: BigDecimal,
            current_amount: BigDecimal,
            description: Option<String>,
        }

        let tokens = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, TokenCreatedRow>(
                r#"
                WITH created_tokens AS (
                    SELECT 
                        t.token_id,
                        t.symbol,
                        t.image_uri,
                        t.name,
                        t.total_supply,
                        t.description,
                        t.created_at,
                        t.creator,
                        t.is_listing,
                        COALESCE(m.price, 0) as price,
                        COALESCE(b.balance, 0) as current_amount,
                        COALESCE(m.price * b.balance, 0) as current_value
                    FROM token t
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                    WHERE t.creator = $1
                )
                SELECT 
                    token_id,
                    symbol,
                    image_uri,
                    name,
                    is_listing,
                    created_at,
                    COALESCE(price, 0) as price,
                    total_supply,
                    COALESCE(price * total_supply, 0) as market_cap,
                    COALESCE(current_amount, 0) as current_amount,
                    description
                FROM created_tokens
                ORDER BY current_value DESC
                LIMIT $2
                OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit as i64)
            .bind(offset)
            .fetch_all(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        // Convert query results to TokenCreated structs
        let tokens: Vec<TokenCreated> = tokens
            .into_iter()
            .map(|row| TokenCreated {
                token: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                is_listing: row.is_listing,
                created_at: row.created_at,
                market_cap: row.market_cap.to_plain_string(),
                total_supply: row.total_supply.to_plain_string(),
                price: row.price.to_plain_string(),
                current_amount: row.current_amount.to_plain_string(),
                description: row.description,
            })
            .collect();

        let total_count = self.fetch_total_count(account_id).await?;

        Ok(TokenCreatedResponse {
            tokens,
            total_count,
        })
    }
}
