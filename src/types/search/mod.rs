use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

use super::common::info::{AccountInfo, TokenInfo};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchToken {
    pub token_info: TokenInfo,
    pub total_supply: BigDecimal,
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
struct SearchAccountRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccount {
    pub account_info: AccountInfo,
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
        let pool = self.db.get_read_pool();

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

        let (token_result, account_result) = tokio::join!(
            // 토큰 검색
            tokio::time::timeout(
                Duration::from_millis(500),
                sqlx::query!(
                    r#"
                    SELECT 
                        t.token_id, t.name, t.symbol, t.image_uri,
                        t.created_at, t.total_supply, m.market_type, m.price
                    FROM token t
                    JOIN market m ON t.token_id = m.token_id
                    
                    WHERE 
                       -- 정확한 매칭 (최우선, 가장 빠름)
                        LOWER(t.token_id) = LOWER($1)
                        OR LOWER(t.name) = LOWER($1)
                        OR LOWER(t.symbol) = LOWER($1)
                        -- Trigram 유사도 매칭 (느리지만 유연함)
                        OR LOWER(t.token_id) % LOWER($1)
                        OR LOWER(t.name) % LOWER($1)
                        OR LOWER(t.symbol) % LOWER($1)
                    ORDER BY 
                        CASE 
                            WHEN LOWER(t.name) = LOWER($1) OR LOWER(t.symbol) = LOWER($1) THEN 0
                            ELSE 1
                        END,
                        m.price DESC
                    LIMIT 50
                    "#,
                    query
                )
                .fetch_all(pool)
            ),
            // 계정 기본 정보
            tokio::time::timeout(
                Duration::from_millis(500),
                sqlx::query_as::<_, SearchAccountRow>(
                    r#"
                    SELECT a.account_id, nickname, image_uri, follower_count, following_count,
                    ax.x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label
                    FROM account a
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    WHERE 
                        LOWER(a.nickname) = LOWER($1)
                        OR LOWER(a.account_id) = LOWER($1)
                        OR LOWER(a.nickname) % LOWER($1)
                        OR LOWER(a.account_id) % LOWER($1)
                    ORDER BY 
                        CASE 
                            WHEN LOWER(a.nickname) = LOWER($1) OR LOWER(a.account_id) = LOWER($1) THEN 0
                            ELSE 1
                        END,
                        follower_count DESC
                    LIMIT 5
                    "#
                )
                .bind(query)
                .fetch_all(pool)
            )
        );

        let token_records = token_result.map_err(|_| anyhow!("Query timeout after 500ms"))??;
        let account_records = account_result.map_err(|_| anyhow!("Query timeout after 500ms"))??;

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
                total_supply: row.total_supply,
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
}
