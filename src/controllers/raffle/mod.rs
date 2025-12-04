use std::sync::Arc;

use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::raffle::{RaffleCheckResponse, RafflePrize, RaffleStatusResponse},
};

pub struct RaffleController {
    pub db: Arc<PostgresDatabase>,
}

impl RaffleController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        RaffleController { db }
    }

    pub async fn get_raffle_status(&self, account_id: &str) -> Result<RaffleStatusResponse> {
        // Get active round
        let active_round = measure_postgres!(
            "raffle.get_active_round",
            sqlx::query!(
                r#"
                SELECT round, status
                FROM raffle_round
                WHERE status = 'ACTIVE'
                LIMIT 1
                "#
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get active round: {}", err))?;

        match active_round {
            Some(round_row) => {
                // Get raffle count for this round
                let count = measure_postgres!(
                    "raffle.get_raffle_count",
                    sqlx::query!(
                        r#"
                        SELECT COUNT(*) as "count!"
                        FROM raffle
                        WHERE round = $1 AND account_id = $2
                        "#,
                        round_row.round,
                        account_id
                    )
                    .fetch_one(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to get raffle count: {}", err))?;

                Ok(RaffleStatusResponse {
                    is_eligible: true,
                    count: count.count,
                })
            }
            None => Ok(RaffleStatusResponse {
                is_eligible: true,
                count: 0,
            }),
        }
    }

    pub async fn check_raffle(&self, round: i64, account_id: &str) -> Result<RaffleCheckResponse> {
        // Get total raffle entries for this round and account
        let total_raffle = measure_postgres!(
            "raffle.get_total_raffle",
            sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!"
                FROM raffle
                WHERE round = $1 AND account_id = $2
                "#,
                round,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get raffle count: {}", err))?;

        // Get winning prizes for this round and account
        let prizes = measure_postgres!(
            "raffle.get_prizes",
            sqlx::query!(
                r#"
                SELECT raffle_id, rank, type as prize_type, transaction_hash, amount
                FROM raffle_winner
                WHERE round = $1 AND account_id = $2
                ORDER BY rank ASC
                "#,
                round,
                account_id
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get prizes: {}", err))?;

        let total_wins = prizes.len() as i64;

        let total_amount: BigDecimal = prizes
            .iter()
            .map(|row| row.amount.clone())
            .sum();

        let prizes: Vec<RafflePrize> = prizes
            .into_iter()
            .map(|row| RafflePrize {
                raffle_id: row.raffle_id,
                rank: row.rank,
                prize_type: row.prize_type,
                transaction_hash: row.transaction_hash,
                amount: row.amount.to_string(),
            })
            .collect();

        Ok(RaffleCheckResponse {
            round,
            account_id: account_id.to_string(),
            total_raffle: total_raffle.count,
            total_wins,
            total_amount: total_amount.to_string(),
            prizes,
        })
    }
}
