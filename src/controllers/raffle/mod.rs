use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::raffle::{RaffleCheckResponse, RafflePrizes, RaffleRound, RaffleRoundResponse, RaffleStatusResponse},
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
        let result = measure_postgres!(
            "raffle.check_raffle",
            sqlx::query!(
                r#"
                SELECT
                    rr.round,
                    rr.start_at,
                    rr.end_at,
                    (SELECT COUNT(*) FROM raffle r WHERE r.round = $1 AND r.account_id = $2) as "raffle_count!",
                    COALESCE(SUM(CASE WHEN rw.type = 'GENERAL_MONAD' THEN rw.amount ELSE 0 END), 0) as "general_monad!",
                    COALESCE(SUM(CASE WHEN rw.type = 'GENERAL_HYPE' THEN rw.amount ELSE 0 END), 0) as "general_hype!",
                    COALESCE(SUM(CASE WHEN rw.type = 'MONAD_AIRDROP_MONAD' THEN rw.amount ELSE 0 END), 0) as "monad_airdrop_monad!",
                    COALESCE(SUM(CASE WHEN rw.type = 'MONAD_AIRDROP_HYPE' THEN rw.amount ELSE 0 END), 0) as "monad_airdrop_hype!"
                FROM raffle_round rr
                LEFT JOIN raffle_winner rw ON rw.round = rr.round AND rw.account_id = $2
                WHERE rr.round = $1
                GROUP BY rr.round, rr.start_at, rr.end_at
                "#,
                round,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check raffle: {}", err))?;

        Ok(RaffleCheckResponse {
            is_validate: result.raffle_count > 0,
            round: RaffleRound {
                start_at: result.start_at,
                round: result.round,
                end_at: result.end_at,
            },
            account_id: account_id.to_string(),
            prizes: RafflePrizes {
                general_monad: result.general_monad.normalized().to_plain_string(),
                general_hype: result.general_hype.normalized().to_plain_string(),
                monad_airdrop_monad: result.monad_airdrop_monad.normalized().to_plain_string(),
                monad_airdrop_hype: result.monad_airdrop_hype.normalized().to_plain_string(),
            },
        })
    }

    pub async fn get_current_round(&self) -> Result<Option<RaffleRoundResponse>> {
        let round = measure_postgres!(
            "raffle.get_current_round",
            sqlx::query!(
                r#"
                SELECT round, status, start_at, end_at
                FROM raffle_round
                WHERE round = (SELECT MAX(round) FROM raffle_round)
                "#
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get current round: {}", err))?;

        Ok(round.map(|r| RaffleRoundResponse {
            round: r.round,
            status: r.status,
            start_at: r.start_at,
            end_at: r.end_at,
        }))
    }
}
