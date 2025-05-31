use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::Utc;
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchAccount {
    pub account_info: AccountInfo,
    pub period: String,
    pub total_profit: BigDecimal,
    pub roi_percentage: BigDecimal,
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
            // 토큰 검색 (기존 유지)
            sqlx::query!(
                r#"
                SELECT 
                    t.token_id, t.name, t.symbol, t.image_uri,
                    t.created_at, t.total_supply, m.market_type, m.price
                FROM token t
                JOIN market m ON t.token_id = m.token_id
                WHERE 
                    LOWER(t.name) = LOWER($1)
                    OR LOWER(t.symbol) = LOWER($1)
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
            .fetch_all(pool),
            // 계정 검색 (손익 계산 완전 제거)
            sqlx::query!(
                r#"
                SELECT 
                    account_id, nickname, image_uri, 
                    follower_count, following_count
                FROM account
                WHERE 
                    LOWER(nickname) = LOWER($1)
                    OR LOWER(account_id) = LOWER($1)
                    OR LOWER(nickname) % LOWER($1)
                    OR LOWER(account_id) % LOWER($1)
                ORDER BY 
                    CASE 
                        WHEN LOWER(nickname) = LOWER($1) OR LOWER(account_id) = LOWER($1) THEN 0
                        ELSE 1
                    END,
                    follower_count DESC
                LIMIT 20
                "#,
                query
            )
            .fetch_all(pool)
        );

        // 토큰 결과 처리
        let token_records = token_result?;
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

        // 계정 결과 처리 (손익 없이)
        let account_records = account_result?;
        let accounts_vec: Vec<SearchAccount> = account_records
            .into_iter()
            .map(|row| SearchAccount {
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.nickname,
                    image_uri: row.image_uri,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                },
                period: "7D".to_string(),
                total_profit: BigDecimal::from(0), // 손익 계산 제거
                roi_percentage: BigDecimal::from(0),
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
