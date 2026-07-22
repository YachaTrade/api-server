use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::AccountInfo,
        leaderboard::{LeaderboardQuery, Pnl, PnlLeaderboardEntry, PnlLeaderboardResponse},
    },
};

#[derive(Debug, sqlx::FromRow)]
struct PnlLeaderboardRow {
    rank: i64,
    account_id: String,
    nickname: String,
    bio: String,
    image_uri: String,
    total_invested_native: BigDecimal,
    total_invested_usd: BigDecimal,
    realized_native: BigDecimal,
    realized_usd: BigDecimal,
    unrealized_native: BigDecimal,
    unrealized_usd: BigDecimal,
    total_count: i64,
    last_updated_at: i64,
}

pub struct LeaderboardController {
    pub db: Arc<PostgresDatabase>,
}

impl LeaderboardController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        LeaderboardController { db }
    }

    pub async fn get_pnl_leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<PnlLeaderboardResponse> {
        let limit = query.limit;
        let offset = (query.page - 1) * limit;

        let rows = measure_postgres!(
            "leaderboard.get_pnl_leaderboard",
            sqlx::query_as::<_, PnlLeaderboardRow>(
                r#"
                WITH ranked AS (
                    SELECT
                        ROW_NUMBER() OVER (ORDER BY (ps.realized_native + ps.unrealized_native) DESC) as rank,
                        ps.account_id,
                        ps.total_invested_native,
                        ps.total_invested_usd,
                        ps.realized_native,
                        ps.realized_usd,
                        ps.unrealized_native,
                        ps.unrealized_usd,
                        COUNT(*) OVER () as total_count,
                        MAX(ps.updated_at) OVER () as last_updated_at
                    FROM pnl_aggregator ps
                )
                SELECT
                    r.rank,
                    r.account_id,
                    a.nickname as nickname,
                    a.bio,
                    a.image_uri as image_uri,
                    r.total_invested_native,
                    r.total_invested_usd,
                    r.realized_native,
                    r.realized_usd,
                    r.unrealized_native,
                    r.unrealized_usd,
                    r.total_count,
                    r.last_updated_at
                FROM ranked r
                JOIN account a ON r.account_id = a.account_id
                ORDER BY r.rank
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch pnl leaderboard: {}", err))?;

        let total_count = rows.first().map(|r| r.total_count).unwrap_or(0);
        let last_updated_at = rows.first().map(|r| r.last_updated_at).unwrap_or(0);

        let ranks = rows
            .into_iter()
            .map(|row| {
                let total_native = &row.realized_native + &row.unrealized_native;
                let total_usd = &row.realized_usd + &row.unrealized_usd;

                let native_percent = if row.total_invested_native > BigDecimal::from(0) {
                    (&total_native / &row.total_invested_native * BigDecimal::from(100))
                        .round(2)
                        .to_string()
                } else {
                    "0".to_string()
                };

                let usd_percent = if row.total_invested_usd > BigDecimal::from(0) {
                    (&total_usd / &row.total_invested_usd * BigDecimal::from(100))
                        .round(2)
                        .to_string()
                } else {
                    "0".to_string()
                };

                PnlLeaderboardEntry {
                    rank: row.rank + offset,
                    account_info: AccountInfo {
                        account_id: row.account_id,
                        nickname: row.nickname,
                        bio: row.bio,
                        image_uri: row.image_uri,
                    },
                    pnl: Pnl {
                        realized_native: row.realized_native.round(0).to_string(),
                        unrealized_native: row.unrealized_native.round(0).to_string(),
                        total_native: total_native.round(0).to_string(),
                        native_percent,
                        realized_usd: row.realized_usd.round(2).to_string(),
                        unrealized_usd: row.unrealized_usd.round(2).to_string(),
                        total_usd: total_usd.round(2).to_string(),
                        usd_percent,
                    },
                }
            })
            .collect();

        Ok(PnlLeaderboardResponse {
            ranks,
            total_count,
            last_updated_at,
        })
    }
}
