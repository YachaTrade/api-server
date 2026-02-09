use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::chester::{
        ChesterInfoResponse, ChesterRewardItem, ChesterRewardsResponse, ChesterVolumeResponse,
    },
};

const COINGECKO_APR_URL: &str =
    "https://api.coingecko.com/api/v3/simple/price?ids=apriori&vs_currencies=usd";

pub struct ChesterController {
    pub db: Arc<PostgresDatabase>,
}

impl ChesterController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ChesterController { db }
    }

    pub async fn get_volume(&self, account_id: &str) -> Result<ChesterVolumeResponse> {
        let volume = measure_postgres!(
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
        .map_err(|err| anyhow!("Failed to get chester volume: {}", err))?;

        let fee = measure_postgres!(
            "chester.get_fee",
            sqlx::query!(
                r#"
                SELECT COALESCE(SUM(ph.value), 0) as "total_fee_usd!"
                FROM point_history ph
                INNER JOIN chester_round cr ON cr.status = 'ACTIVE'
                WHERE ph.account_id = $1
                  AND ph.created_at >= cr.start_at
                  AND ph.created_at <= cr.end_at
                "#,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester fee: {}", err))?;

        Ok(ChesterVolumeResponse {
            volume_usd: volume.total_usd_volume.normalized().to_plain_string(),
            fee_usd: fee.total_fee_usd.normalized().to_plain_string(),
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
        // 1. Fetch raw reward rows with token info
        let rows = measure_postgres!(
            "chester.get_rewards",
            sqlx::query!(
                r#"
                SELECT rw.token_id, rw.amount, crt.name, crt.symbol, crt.image_uri
                FROM chester_reward rw
                INNER JOIN chester_round cr ON cr.round = rw.round AND cr.status = 'ACTIVE'
                INNER JOIN chester_reward_token crt ON crt.token_id = rw.token_id
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

        // 3. Fetch APR/USD price from CoinGecko
        let apr_usd = Self::fetch_apr_usd().await;

        // 4. Calculate USD values
        let decimals = BigDecimal::from(10u64.pow(18));

        let rewards = rows
            .into_iter()
            .map(|r| {
                // MON → price table, APR → CoinGecko
                let price = if r.symbol == "MON" {
                    mon_usd.clone()
                } else {
                    apr_usd.clone()
                };
                let usd_value = &r.amount * &price / &decimals;

                ChesterRewardItem {
                    token_id: r.token_id,
                    name: r.name,
                    symbol: r.symbol,
                    image_uri: r.image_uri,
                    amount: r.amount.normalized().to_plain_string(),
                    price: price.normalized().to_plain_string(),
                    usd_value: usd_value.normalized().to_plain_string(),
                }
            })
            .collect();

        Ok(ChesterRewardsResponse { rewards })
    }

    async fn fetch_apr_usd() -> BigDecimal {
        let res = match reqwest::get(COINGECKO_APR_URL).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("CoinGecko APR fetch failed: {}", e);
                return BigDecimal::from(0);
            }
        };
        let data: HashMap<String, HashMap<String, f64>> = match res.json().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("CoinGecko APR parse failed: {}", e);
                return BigDecimal::from(0);
            }
        };
        data.get("apriori")
            .and_then(|m| m.get("usd").copied())
            .and_then(|v| BigDecimal::from_str(&v.to_string()).ok())
            .unwrap_or(BigDecimal::from(0))
    }
}
