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
        // 검색 패턴을 소문자로 변환한 후 와일드카드 적용
        let search_pattern = format!("%{}%", query.to_lowercase());
        let pool = self.db.get_read_pool();

        // 현재 시간 및 7일 전 타임스탬프 계산
        let now = Utc::now().timestamp();
        let seven_days_ago = now - (7 * 24 * 60 * 60);

        // 토큰 검색 쿼리와 계정 ID 검색 쿼리를 동시에 실행합니다.
        let token_query = sqlx::query!(
            r#"
            SELECT 
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri,
                t.created_at,
                t.total_supply,
                m.market_type,
                m.price
            FROM token t
            JOIN market m ON t.token_id = m.token_id
            WHERE 
                LOWER(t.token_id) LIKE $1
                OR LOWER(t.name) LIKE $1
                OR LOWER(t.symbol) LIKE $1
            "#,
            search_pattern
        )
        .fetch_all(pool);

        let account_id_query = sqlx::query!(
            r#"
            SELECT account_id
            FROM account
            WHERE LOWER(account_id) LIKE $1
               OR LOWER(nickname) LIKE $1
            "#,
            search_pattern
        )
        .fetch_optional(pool);

        let (token_records_res, account_id_res) = tokio::join!(token_query, account_id_query);

        // 토큰 쿼리 결과 처리
        let token_records = token_records_res.map_err(|err| anyhow::anyhow!(err))?;
        let tokens_vec: Vec<SearchToken> = token_records
            .into_iter()
            .map(|row| SearchToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                total_supply: row.total_supply,
                price: row.price,
                created_at: row.created_at,
            })
            .collect();

        let tokens = SearchTokenResponse {
            total_count: tokens_vec.len() as i64,
            tokens: tokens_vec,
        };

        // 계정 관련 쿼리: 계정 ID 결과가 있는 경우에만 추가 쿼리를 실행합니다.
        let accounts_vec =
            if let Some(account_row) = account_id_res.map_err(|err| anyhow::anyhow!(err))? {
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
                        p.account_id,
                        SUM(p.total_bought_native) AS total_cost,
                        (SUM(p.realized_pnl) + SUM(
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
                        )) AS total_profit
                    FROM position p            
                    JOIN market m ON p.token_id = m.token_id
                    WHERE p.created_at >= $2
                    GROUP BY p.account_id
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
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

                account_records
                    .into_iter()
                    .map(|row| {
                        let roi_percentage = row.total_profit.clone() / row.total_cost;
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
