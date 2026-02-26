use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    config::{APR_CONTRACT_ADDRESS, MON_CONTRACT_ADDRESS},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        chester::{
            ChesterBoxRewardItem, ChesterBoxRewardsResponse, ChesterInfoResponse,
            ChesterRewardItem, ChesterRewardsResponse, ChesterVolumeResponse,
        },
        common::info::{AccountInfo, SwapInfo, SwapType, TokenInfo, TokenSwapInfo},
        profile::SwapHistoryResponse,
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
        let mon_addr = MON_CONTRACT_ADDRESS.as_str();
        let apr_addr = APR_CONTRACT_ADDRESS.as_str();

        let rewards = rows
            .into_iter()
            .map(|r| {
                let price = if r.token_id == mon_addr {
                    mon_usd.clone()
                } else if r.token_id == apr_addr {
                    apr_usd.clone()
                } else {
                    BigDecimal::from(0)
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
            .collect::<Vec<_>>();

        // Sort by usd_value descending
        let mut rewards = rewards;
        rewards.sort_by(|a, b| {
            let a_val = BigDecimal::from_str(&a.usd_value).unwrap_or(BigDecimal::from(0));
            let b_val = BigDecimal::from_str(&b.usd_value).unwrap_or(BigDecimal::from(0));
            b_val.cmp(&a_val)
        });

        Ok(ChesterRewardsResponse { rewards })
    }

    pub async fn get_box_rewards(
        &self,
        account_id: &str,
        round: Option<i64>,
    ) -> Result<ChesterBoxRewardsResponse> {
        // If round not specified, get the latest round
        let target_round = match round {
            Some(r) => r,
            None => {
                let row = measure_postgres!(
                    "chester.get_latest_round",
                    sqlx::query!(
                        r#"
                        SELECT round as "round!"
                        FROM chester_round
                        ORDER BY round DESC
                        LIMIT 1
                        "#
                    )
                    .fetch_optional(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to get latest chester round: {}", err))?;

                match row {
                    Some(r) => r.round,
                    None => return Ok(ChesterBoxRewardsResponse { rewards: vec![] }),
                }
            }
        };

        let rows = measure_postgres!(
            "chester.get_box_rewards",
            sqlx::query!(
                r#"
                SELECT
                    cbr.round,
                    cbr.level,
                    cbr.token_id,
                    crt.name,
                    crt.symbol,
                    crt.image_uri,
                    cbr.amount,
                    cbr.status,
                    cbr.proof,
                    cbr.transaction_hash,
                    cbr.claimed_at
                FROM chester_box_reward cbr
                JOIN chester_reward_token crt ON crt.token_id = cbr.token_id
                WHERE cbr.account_id = $1
                  AND cbr.round = $2
                ORDER BY cbr.level ASC, cbr.amount DESC
                "#,
                account_id,
                target_round
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester box rewards: {}", err))?;

        let rewards = rows
            .into_iter()
            .map(|r| ChesterBoxRewardItem {
                round: r.round,
                level: r.level,
                token_id: r.token_id,
                name: r.name,
                symbol: r.symbol,
                image_uri: r.image_uri,
                amount: r.amount.normalized().to_plain_string(),
                status: r.status,
                proof: r.proof,
                transaction_hash: r.transaction_hash,
                claimed_at: r.claimed_at,
            })
            .collect();

        Ok(ChesterBoxRewardsResponse { rewards })
    }

    pub async fn get_swap_history(
        &self,
        account_id: &str,
        page: i64,
        limit: i64,
    ) -> Result<SwapHistoryResponse> {
        let offset = (page - 1) * limit;

        #[derive(sqlx::FromRow)]
        struct SwapRow {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image_uri: String,
            token_description: Option<String>,
            token_twitter: Option<String>,
            token_telegram: Option<String>,
            token_website: Option<String>,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            token_created_at: i64,
            creator: String,
            creator_nickname: String,
            creator_image_uri: String,
            creator_bio: String,
            is_buy: bool,
            native_amount: BigDecimal,
            token_amount: BigDecimal,
            native_price: BigDecimal,
            value: BigDecimal,
            created_at: i64,
            transaction_hash: String,
        }

        let rows = measure_postgres!(
            "chester.get_swap_history",
            sqlx::query_as::<_, SwapRow>(
                r#"
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                recent_swaps AS (
                    SELECT
                        s.token_id,
                        s.is_buy,
                        s.native_amount,
                        s.token_amount,
                        s.value,
                        s.created_at,
                        s.transaction_hash
                    FROM swap s
                    INNER JOIN chester_round cr ON cr.status = 'ACTIVE'
                    WHERE s.account_id = $1
                      AND s.created_at >= cr.start_at
                      AND s.created_at <= cr.end_at
                    ORDER BY s.block_number DESC, s.tx_index DESC, s.log_index DESC
                    LIMIT $2
                    OFFSET $3
                )
                SELECT
                    t.token_id,
                    t.name as token_name,
                    t.symbol as token_symbol,
                    t.image_uri as token_image_uri,
                    t.description as token_description,
                    t.twitter as token_twitter,
                    t.telegram as token_telegram,
                    t.website as token_website,
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                    t.created_at as token_created_at,
                    t.creator,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.bio as creator_bio,
                    rs.is_buy,
                    rs.native_amount,
                    rs.token_amount,
                    rs.value,
                    COALESCE(lp.price, 0) as native_price,
                    rs.created_at,
                    rs.transaction_hash
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                CROSS JOIN latest_price lp
                ORDER BY rs.created_at DESC
                "#,
            )
            .bind(account_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get chester swap history: {}", err))?;

        let total_count = if rows.is_empty() {
            0
        } else {
            measure_postgres!(
                "chester.get_swap_history_count",
                sqlx::query!(
                    r#"
                    SELECT COUNT(*) as "count!"
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
            .map_err(|err| anyhow!("Failed to get chester swap history count: {}", err))?
            .count
        };

        let swaps = rows
            .into_iter()
            .map(|row| TokenSwapInfo {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.token_name,
                    symbol: row.token_symbol,
                    image_uri: row.token_image_uri,
                    description: row.token_description,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: row.token_twitter,
                    telegram: row.token_telegram,
                    website: row.token_website,
                    created_at: row.token_created_at,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                    },
                    is_cto: row.is_cto,
                    hackathon_info: None,
                },
                swap_info: SwapInfo {
                    event_type: if row.is_buy {
                        SwapType::Buy
                    } else {
                        SwapType::Sell
                    },
                    native_amount: row.native_amount.normalized().to_plain_string(),
                    token_amount: row.token_amount.normalized().to_plain_string(),
                    native_price: row.native_price.normalized().to_plain_string(),
                    value: row.value.normalized().to_plain_string(),
                    transaction_hash: row.transaction_hash,
                    created_at: row.created_at,
                },
            })
            .collect();

        Ok(SwapHistoryResponse { swaps, total_count })
    }

    async fn fetch_apr_usd() -> BigDecimal {
        let client = reqwest::Client::new();
        let body = match client.get(COINGECKO_APR_URL).header("User-Agent", "nad.fun").send().await {
            Ok(r) => match r.text().await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("CoinGecko APR read failed: {}", e);
                    return BigDecimal::from(0);
                }
            },
            Err(e) => {
                tracing::warn!("CoinGecko APR fetch failed: {}", e);
                return BigDecimal::from(0);
            }
        };
        let data: HashMap<String, HashMap<String, f64>> = match serde_json::from_str(&body) {
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
