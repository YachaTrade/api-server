use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    config::WMON,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::chester::{
        ChesterInfoResponse, ChesterRewardItem, ChesterRewardsResponse,
        ChesterVolumeResponse,
    },
};

pub struct ChesterController {
    pub db: Arc<PostgresDatabase>,
}

impl ChesterController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ChesterController { db }
    }

    pub async fn get_volume(&self, account_id: &str) -> Result<ChesterVolumeResponse> {
        let result = measure_postgres!(
            "chester.get_volume",
            sqlx::query!(
                r#"
                SELECT COALESCE(SUM(s.value), 0) as "total_usd_volume!"
                FROM swap s
                INNER JOIN chester_round cr ON cr.status = 'ACTIVE'
                WHERE s.account_id = $1
                  AND s.created_at >= cr.start_at
                  AND s.created_at <= cr.end_at
                "#,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester round volume: {}", err))?;

        Ok(ChesterVolumeResponse {
            total_usd_volume: result.total_usd_volume.normalized().to_plain_string(),
        })
    }

    pub async fn get_round(&self) -> Result<Option<ChesterInfoResponse>> {
        let round = measure_postgres!(
            "chester.get_round",
            sqlx::query!(
                r#"
                SELECT round, start_at, end_at, status
                FROM chester_round
                WHERE status = 'ACTIVE'
                "#
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester round: {}", err))?;

        Ok(round.map(|r| ChesterInfoResponse {
            round: r.round,
            start_at: r.start_at,
            end_at: r.end_at,
            status: r.status,
            chest_level_threshold: crate::types::chester::chest_level_threshold(),
        }))
    }

    pub async fn get_rewards(&self) -> Result<ChesterRewardsResponse> {
        // 1. Fetch raw reward rows
        let rows = measure_postgres!(
            "chester.get_rewards",
            sqlx::query!(
                r#"
                SELECT rw.token_id, rw.amount
                FROM chester_reward rw
                INNER JOIN chester_round cr ON cr.round = rw.round AND cr.status = 'ACTIVE'
                "#
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester round rewards: {}", err))?;

        // 2. Fetch latest MON/USD price
        let mon_usd = measure_postgres!(
            "chester.get_mon_price",
            sqlx::query!(
                r#"
                SELECT price FROM price ORDER BY block_number DESC LIMIT 1
                "#
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get MON/USD price: {}", err))?
        .map(|r| r.price)
        .unwrap_or(BigDecimal::from(0));

        // 3. Fetch market prices (token → MON)
        let token_ids: Vec<String> = rows.iter().map(|r| r.token_id.clone()).collect();
        let markets = measure_postgres!(
            "chester.get_market_prices",
            sqlx::query!(
                r#"
                SELECT token_id, price FROM market WHERE token_id = ANY($1)
                "#,
                &token_ids
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get market prices: {}", err))?;

        let market_map: std::collections::HashMap<String, BigDecimal> = markets
            .into_iter()
            .map(|m| (m.token_id.to_lowercase(), m.price))
            .collect();

        let wmon = WMON.to_lowercase();

        // 4. Calculate USD values in Rust
        let rewards = rows
            .into_iter()
            .map(|r| {
                let usd_value = if r.token_id.to_lowercase() == wmon {
                    // WMON: amount * MON/USD price
                    &r.amount * &mon_usd
                } else {
                    // Others: amount * market_price(token→MON) * MON/USD
                    let market_price = market_map
                        .get(&r.token_id.to_lowercase())
                        .cloned()
                        .unwrap_or(BigDecimal::from(0));
                    &r.amount * &market_price * &mon_usd
                };

                ChesterRewardItem {
                    token_id: r.token_id,
                    amount: r.amount.normalized().to_plain_string(),
                    usd_value: usd_value.normalized().to_plain_string(),
                }
            })
            .collect();

        Ok(ChesterRewardsResponse { rewards })
    }
}
