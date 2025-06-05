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

        let now = Utc::now().timestamp();
        let seven_days_ago = now - (7 * 24 * 60 * 60);

        let (token_result, account_basic_result) = tokio::join!(
            // 토큰 검색
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
            .fetch_all(pool),
            // 계정 기본 정보
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
        );

        let token_records = token_result?;
        let account_basic_records = account_basic_result?;
        let account_ids: Vec<String> = account_basic_records
            .iter()
            .map(|r| r.account_id.clone())
            .collect();

        // position 테이블 사용 (balance 대신)
        let profit_data = if !account_ids.is_empty() {
            sqlx::query!(
                r#"
                SELECT 
                    p.account_id,
                    COALESCE(SUM(p.total_bought_native), 0)::numeric as "total_cost!: BigDecimal",
                    COALESCE((SUM(p.realized_pnl) + SUM(
                        COALESCE(
                            CASE 
                                WHEN p.current_token_amount = 0 THEN 0
                                WHEN m.market_type = 'CURVE' THEN
                                    m.virtual_native 
                                    - (
                                        ((m.virtual_token * m.virtual_native) 
                                        + (m.virtual_token + p.current_token_amount) - 1)
                                        / (m.virtual_token + p.current_token_amount)
                                    )
                                WHEN m.market_type = 'DEX' THEN
                                    m.price * p.current_token_amount
                                ELSE 0
                            END,
                        0)
                    )), 0)::numeric as "total_profit!: BigDecimal"
                FROM position p  
                JOIN market m ON p.token_id = m.token_id
                WHERE p.account_id = ANY($1) AND p.created_at >= $2
                GROUP BY p.account_id
                "#,
                &account_ids,
                seven_days_ago
            )
            .fetch_all(pool)
            .await?
        } else {
            vec![]
        };

        // 결과 조합 (기존과 동일)
        let profit_map: std::collections::HashMap<String, (BigDecimal, BigDecimal)> = profit_data
            .into_iter()
            .map(|row| (row.account_id, (row.total_cost, row.total_profit)))
            .collect();

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

        let accounts_vec: Vec<SearchAccount> = account_basic_records
            .into_iter()
            .map(|row| {
                let (total_cost, total_profit) = profit_map
                    .get(&row.account_id)
                    .cloned()
                    .unwrap_or((BigDecimal::from(0), BigDecimal::from(0)));

                let roi_percentage = if total_cost == BigDecimal::from(0) {
                    BigDecimal::from(0)
                } else {
                    total_profit.clone() / total_cost.clone()
                };

                SearchAccount {
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
                    period: "7D".to_string(),
                    total_profit,
                    roi_percentage,
                }
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
