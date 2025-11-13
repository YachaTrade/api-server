use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::raffle::{Prize, PrizeListResponse, RaffleStatusResponse},
};

pub struct RaffleController {
    pub db: Arc<PostgresDatabase>,
}

impl RaffleController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        RaffleController { db }
    }

    pub async fn get_raffle_status(&self, account_id: &str) -> Result<RaffleStatusResponse> {
        // Check if account is in monad_airdrop
        let is_eligible = measure_postgres!(
            "raffle.check_monad_airdrop",
            sqlx::query!(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM monad_airdrop WHERE account_id = $1
                ) as "exists!"
                "#,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check monad_airdrop: {}", err))?
        .exists;

        if !is_eligible {
            return Ok(RaffleStatusResponse {
                is_eligible: false,
                count: 0,
            });
        }

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

    pub async fn get_prize_list(&self, round: i64) -> Result<PrizeListResponse> {
        let prizes = measure_postgres!(
            "raffle.get_prize_list",
            sqlx::query!(
                r#"
                SELECT account_id, amount
                FROM prize
                WHERE round = $1
                ORDER BY amount DESC
                "#,
                round
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get prize list: {}", err))?;

        let prizes = prizes
            .into_iter()
            .map(|row| Prize {
                account_id: row.account_id,
                amount: row.amount,
            })
            .collect();

        Ok(PrizeListResponse { prizes })
    }
}
