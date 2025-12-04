use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::raffle::{RaffleCheckResponse, RafflePrizes, RaffleStatusResponse},
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
        let prizes = measure_postgres!(
            "raffle.get_prizes",
            sqlx::query!(
                r#"
                SELECT
                    COALESCE(SUM(CASE WHEN type = 'GENERAL_MONAD' THEN amount ELSE 0 END), 0) as "general_monad!",
                    COALESCE(SUM(CASE WHEN type = 'GENERAL_HYPE' THEN amount ELSE 0 END), 0) as "general_hype!",
                    COALESCE(SUM(CASE WHEN type = 'MONAD_AIRDROP_MONAD' THEN amount ELSE 0 END), 0) as "monad_airdrop_monad!",
                    COALESCE(SUM(CASE WHEN type = 'MONAD_AIRDROP_HYPE' THEN amount ELSE 0 END), 0) as "monad_airdrop_hype!"
                FROM raffle_winner
                WHERE round = $1 AND account_id = $2
                "#,
                round,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get prizes: {}", err))?;

        Ok(RaffleCheckResponse {
            round,
            account_id: account_id.to_string(),
            prizes: RafflePrizes {
                general_monad: prizes.general_monad.normalized().to_plain_string(),
                general_hype: prizes.general_hype.normalized().to_plain_string(),
                monad_airdrop_monad: prizes.monad_airdrop_monad.normalized().to_plain_string(),
                monad_airdrop_hype: prizes.monad_airdrop_hype.normalized().to_plain_string(),
            },
        })
    }
}
