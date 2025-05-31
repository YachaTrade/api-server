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

        // 빈 검색어 처리
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

        let now = Utc::now().timestamp();
        let seven_days_ago = now - (7 * 24 * 60 * 60);

        // 토큰과 계정 검색을 병렬로 실행
        let (token_result, account_result) = tokio::join!(
            // 토큰 검색
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
                    OR LOWER(t.token_id) = LOWER($1)
                    OR LOWER(t.name) % LOWER($1)
                    OR LOWER(t.symbol) % LOWER($1)
                    OR LOWER(t.token_id) % LOWER($1)
                ORDER BY 
                    CASE 
                        WHEN LOWER(t.name) = LOWER($1) 
                          OR LOWER(t.symbol) = LOWER($1)
                          OR LOWER(t.token_id) = LOWER($1) THEN 0
                        ELSE 1
                    END,
                    m.price DESC
                LIMIT 50
                "#,
                query
            )
            .fetch_optional(pool),
            // 계정 검색 (첫 번째 매칭만)
            sqlx::query!(
                r#"
                SELECT account_id
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
                LIMIT 1
                "#,
                query
            )
            .fetch_optional(pool)
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

        let tokens = SearchTokenResponse {
            total_count: tokens_vec.len() as i64,
            tokens: tokens_vec,
        };

        // 계정 결과 처리
        let accounts_vec = if let Some(account_row) = account_result? {
            let account_records = sqlx::query!(
                r#"
                SELECT 
                    a.account_id,
                    a.nickname,
                    a.image_uri,
                    a.follower_count,
                    a.following_count,
                    COALESCE(ps.total_cost, 0)::numeric AS "total_cost!: BigDecimal",
                    COALESCE(ps.total_profit, 0)::numeric AS "total_profit!: BigDecimal"
                FROM account a
                LEFT JOIN (
                    SELECT 
                        b.account_id,
                        SUM(p.total_bought_native) AS total_cost,
                        (SUM(p.total_sold_native - ((p.total_bought_native / p.total_bought_token) * p.total_sold_token)) + SUM(
                            COALESCE(
                                CASE 
                                    WHEN b.balance = 0 THEN 0
                                    WHEN m.market_type = 'CURVE' THEN
                                        m.virtual_native 
                                        - (
                                            ((m.virtual_token * m.virtual_native) 
                                            + (m.virtual_token + b.balance) - 1)
                                            / (m.virtual_token + b.balance)
                                        )
                                    WHEN m.market_type = 'DEX' THEN
                                        m.price * b.balance
                                    ELSE 0
                                END,
                            0)
                        )) AS total_profit
                    FROM balance b
                    JOIN positions p ON b.account_id = p.account_id AND b.token_id = p.token_id
                    JOIN market m ON p.token_id = m.token_id
                    WHERE p.created_at >= $2
                    GROUP BY b.account_id
                ) ps ON a.account_id = ps.account_id
                WHERE a.account_id = $1
                ORDER BY 
                    (
                        COALESCE(ps.total_profit, 0)
                        / NULLIF(COALESCE(ps.total_cost, 0), 0)
                    ) DESC
                "#,
                account_row.account_id,
                seven_days_ago,
            )
            .fetch_all(pool)
            .await?;

            account_records
                .into_iter()
                .map(|row| {
                    let roi_percentage = if row.total_cost == BigDecimal::from(0) {
                        BigDecimal::from(0)
                    } else {
                        row.total_profit.clone() / row.total_cost
                    };
                    SearchAccount {
                        account_info: AccountInfo {
                            account_id: row.account_id,
                            nickname: row.nickname,
                            image_uri: row.image_uri,
                            follower_count: row.follower_count,
                            following_count: row.following_count,
                        },
                        period: "7D".to_string(),
                        total_profit: row.total_profit,
                        roi_percentage,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let accounts = SearchAccountResponse {
            total_count: accounts_vec.len() as i64,
            accounts: accounts_vec,
        };

        Ok(SearchResponse { accounts, tokens })
    }
}
