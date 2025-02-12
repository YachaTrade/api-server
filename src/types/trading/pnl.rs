use std::sync::Arc;

use crate::{db::postgres::PostgresDatabase, types::common::info::TokenInfo};
use anyhow::Result;
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPnL {
    pub token_id: String,
    pub token_symbol: String,
    pub token_image: String,
    pub current_price: f64,        // 현재 가격
    pub current_token_amount: f64, // 현재 보유 수량
    pub total_cost: f64,           // 총 매수 비용
    pub current_value: f64,        // 현재 가치
    pub realized_pnl: f64,         // 실현 손익
    pub unrealized_pnl: f64,       // 미실현 손익
    pub total_pnl: f64,            // 총 손익
    pub roi_percentage: f64,       // 투자수익률 (%)
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PeriodPnL {
    pub period: String,      // "7D" or "ALL"
    pub total_profit: f64,   // 총 수익 (realized_pnl + unrealized_pnl)
    pub roi_percentage: f64, // 기간 내 투자수익률
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BestTrade {
    pub token: TokenInfo,
    pub total_profit: f64,   // 현재 수익 (realized_pnl + unrealized_pnl)
    pub roi_percentage: f64, // 투자수익률
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PNLResponse {
    pub last_7d: PeriodPnL,            // 최근 7일 PNL
    pub total: PeriodPnL,              // 전체 기간 PNL
    pub best_trade: Option<BestTrade>, // 현재 가장 큰 수익을 보고 있는 포지션
}

pub struct PNLController {
    pub db: Arc<PostgresDatabase>,
}

impl PNLController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PNLController { db }
    }
    pub async fn get_pnl(&self, account_id: &str) -> Result<PNLResponse> {
        // Get current timestamp and 7 days ago timestamp
        let now = Utc::now().timestamp();
        let seven_days_ago = now - (7 * 24 * 60 * 60); // 7 days in seconds

        // Get positions for different periods
        let seven_days_pnl = self
            .get_period_pnl(account_id, Some(seven_days_ago))
            .await?;
        let total_pnl = self.get_period_pnl(account_id, None).await?;
        let best_trade = self.get_best_trade(account_id).await?;

        Ok(PNLResponse {
            last_7d: seven_days_pnl,
            total: total_pnl,
            best_trade,
        })
    }

    /// Get PNL information for a specific period
    async fn get_period_pnl(&self, account_id: &str, start_time: Option<i64>) -> Result<PeriodPnL> {
        let positions = sqlx::query!(
            r#"
            WITH position_stats AS (
                SELECT 
                    SUM(p.total_bought_native) as total_cost,
                    SUM(p.realized_pnl) as realized_pnl,
                    SUM(
                        COALESCE(
                            CASE 
                                WHEN p.current_token_amount = 0 THEN 0
                                WHEN m.market_type = 'CURVE' THEN
                                    (
                                        m.virtual_native 
                                        - (
                                            ((m.virtual_token * m.virtual_native) 
                                            + (m.virtual_token + p.current_token_amount))
                                            / (m.virtual_token + p.current_token_amount)
                                        )
                                    )
                                    - (p.total_bought_native * p.current_token_amount / p.total_bought_token)
                                WHEN m.market_type = 'DEX' THEN
                                    (
                                        m.reserve_native 
                                        - (
                                            ((m.reserve_token * m.reserve_native)
                                            + (m.reserve_token + p.current_token_amount))
                                            / (m.reserve_token + p.current_token_amount)
                                        )
                                    )
                                    - (p.total_bought_native * p.current_token_amount / p.total_bought_token)
                                ELSE 0
                            END,
                        0)
                    ) as unrealized_pnl
                FROM position p
                JOIN market m ON p.token_id = m.token_id
                WHERE p.account_id = $1
                    AND ($2::bigint IS NULL OR p.created_at >= $2)
            )
            SELECT 
                COALESCE(total_cost, 0)::numeric as "total_cost!: BigDecimal",
                COALESCE(realized_pnl, 0)::numeric as "realized_pnl!: BigDecimal",
                COALESCE(unrealized_pnl, 0)::numeric as "unrealized_pnl!: BigDecimal"
            FROM position_stats
            "#,
            account_id,
            start_time
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        let total_profit = positions.realized_pnl.to_f64().unwrap_or(0.0)
            + positions.unrealized_pnl.to_f64().unwrap_or(0.0);

        let roi_percentage: f64 = if positions.total_cost.to_f64().unwrap_or(0.0) > 0.0 {
            (total_profit / positions.total_cost.to_f64().unwrap_or(1.0)) * 100.0
        } else {
            0.0
        };

        Ok(PeriodPnL {
            period: if start_time.is_some() {
                "7D".to_string()
            } else {
                "ALL".to_string()
            },
            total_profit,
            roi_percentage,
        })
    }

    /// Get the best performing trade
    async fn get_best_trade(&self, account_id: &str) -> Result<Option<BestTrade>> {
        let positions = sqlx::query!(
            r#"
            WITH position_stats AS (
                SELECT 
                    p.token_id,
                    t.symbol as token_symbol,
                    t.image_uri as token_image,
                    t.name as token_name,
                    p.total_bought_native as total_cost,
                    p.realized_pnl,
                    COALESCE(
                            CASE 
                                WHEN p.current_token_amount = 0 THEN 0
                                WHEN m.market_type = 'CURVE' THEN
                                    (
                                        m.virtual_native 
                                        - (
                                            ((m.virtual_token * m.virtual_native) 
                                            + (m.virtual_token + p.current_token_amount))
                                            / (m.virtual_token + p.current_token_amount)
                                        )
                                    )
                                    - (p.total_bought_native * p.current_token_amount / p.total_bought_token)
                                WHEN m.market_type = 'DEX' THEN
                                    (
                                        m.reserve_native 
                                        - (
                                            (m.reserve_token * m.reserve_native)
                                            + (m.reserve_token + p.current_token_amount)
                                            / (m.reserve_token + p.current_token_amount)
                                        )
                                    )
                                    - (p.total_bought_native * p.current_token_amount / p.total_bought_token)
                                ELSE 0
                            END,
                        0) AS unrealized_pnl
                FROM position p
                JOIN token t ON p.token_id = t.token_id
                JOIN market m ON p.token_id = m.token_id
                WHERE p.account_id = $1
            )
            SELECT 
                token_id,
                token_symbol,
                token_image,
                token_name,
                total_cost::numeric as "total_cost!: BigDecimal",
                realized_pnl::numeric as "realized_pnl!: BigDecimal",
                unrealized_pnl::numeric as "unrealized_pnl!: BigDecimal"
            FROM position_stats
            ORDER BY (realized_pnl + unrealized_pnl) DESC
            LIMIT 1
            "#,
            account_id
        )
        .fetch_optional(self.db.get_read_pool())
        .await?;

        Ok(positions.map(|pos| {
            let total_profit = pos.realized_pnl.to_f64().unwrap_or(0.0)
                + pos.unrealized_pnl.to_f64().unwrap_or(0.0);

            let roi_percentage = if pos.total_cost.to_f64().unwrap_or(0.0) > 0.0 {
                (total_profit / pos.total_cost.to_f64().unwrap_or(1.0)) * 100.0
            } else {
                0.0
            };

            BestTrade {
                token: TokenInfo {
                    token_id: pos.token_id,
                    symbol: pos.token_symbol,
                    image_uri: pos.token_image,
                    name: pos.token_name,
                },
                total_profit,
                roi_percentage,
            }
        }))
    }
}
