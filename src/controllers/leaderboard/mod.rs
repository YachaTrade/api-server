use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::AccountInfo,
        leaderboard::{
            HypePointLeaderboardEntry, HypePointLeaderboardResponse, LeaderboardQuery,
            Pnl, PnlLeaderboardEntry, PnlLeaderboardResponse,
        },
    },
};

#[derive(Debug, sqlx::FromRow)]
struct HypePointLeaderboardRow {
    rank: i64,
    account_id: String,
    nickname: String,
    bio: String,
    image_uri: String,
    hype_point: i64,
    total_count: i64,
    total_hype_point: BigDecimal,
}

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
    total_count: i64,
}

pub struct LeaderboardController {
    pub db: Arc<PostgresDatabase>,
}

impl LeaderboardController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        LeaderboardController { db }
    }

    pub async fn get_hype_point_leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<HypePointLeaderboardResponse> {
        let limit = query.limit;
        let offset = query.offset;

        let rows = measure_postgres!(
            "leaderboard.get_hype_point_leaderboard",
            sqlx::query_as::<_, HypePointLeaderboardRow>(
                r#"
                WITH stats AS (
                    SELECT
                        (SELECT total_count FROM hype_point_leaderboard_count WHERE id = 1) as total_count,
                        (SELECT hype_point FROM total_hype_point WHERE id = 1) as total_hype_point
                )
                SELECT
                    ROW_NUMBER() OVER (ORDER BY p.hype_point DESC) as rank,
                    a.account_id,
                    COALESCE(ax.x_handle, a.nickname) as nickname,
                    a.bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    p.hype_point,
                    s.total_count,
                    s.total_hype_point::NUMERIC as total_hype_point
                FROM point p
                JOIN account a ON p.account_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                CROSS JOIN stats s
                WHERE p.hype_point > 0
                ORDER BY p.hype_point DESC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch hype point leaderboard: {}", err))?;

        let (total_count, total_hype_point) = if let Some(first) = rows.first() {
            (
                first.total_count,
                first.total_hype_point.normalized().to_plain_string(),
            )
        } else {
            (0, "0".to_string())
        };

        let last_updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let ranks = rows
            .into_iter()
            .map(|row| HypePointLeaderboardEntry {
                rank: row.rank + offset,
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.nickname,
                    bio: row.bio,
                    image_uri: row.image_uri,
                },
                hype_point: row.hype_point.to_string(),
            })
            .collect();

        Ok(HypePointLeaderboardResponse {
            ranks,
            total_count,
            total_hype_point,
            last_updated_at,
        })
    }

    pub async fn get_pnl_leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<PnlLeaderboardResponse> {
        let limit = query.limit;
        let offset = query.offset;

        let rows = measure_postgres!(
            "leaderboard.get_pnl_leaderboard",
            sqlx::query_as::<_, PnlLeaderboardRow>(
                r#"
                WITH position_agg AS (
                    SELECT
                        account_id,
                        SUM(native_out) as total_invested_native,
                        SUM(usd_out) as total_invested_usd,
                        SUM(native_in - native_out) as position_pnl_native,
                        SUM(usd_in - usd_out) as position_pnl_usd
                    FROM position
                    GROUP BY account_id
                ),
                fee_agg AS (
                    SELECT
                        account_id,
                        SUM(native_amount) as total_fee_native,
                        SUM(usd_amount) as total_fee_usd
                    FROM fee
                    GROUP BY account_id
                ),
                pnl_data AS (
                    SELECT
                        p.account_id,
                        p.total_invested_native,
                        p.total_invested_usd,
                        p.position_pnl_native + COALESCE(f.total_fee_native, 0) as realized_native,
                        p.position_pnl_usd + COALESCE(f.total_fee_usd, 0) as realized_usd
                    FROM position_agg p
                    LEFT JOIN fee_agg f ON p.account_id = f.account_id
                ),
                ranked AS (
                    SELECT
                        ROW_NUMBER() OVER (ORDER BY realized_native DESC) as rank,
                        pnl_data.*,
                        COUNT(*) OVER () as total_count
                    FROM pnl_data
                )
                SELECT
                    r.rank,
                    r.account_id,
                    COALESCE(ax.x_handle, a.nickname) as nickname,
                    a.bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    r.total_invested_native,
                    r.total_invested_usd,
                    r.realized_native,
                    r.realized_usd,
                    r.total_count
                FROM ranked r
                JOIN account a ON r.account_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
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

        let ranks = rows
            .into_iter()
            .map(|row| {
                let native_percent = if row.total_invested_native > BigDecimal::from(0) {
                    (&row.realized_native / &row.total_invested_native * BigDecimal::from(100))
                        .round(2)
                        .to_string()
                } else {
                    "0".to_string()
                };

                let usd_percent = if row.total_invested_usd > BigDecimal::from(0) {
                    (&row.realized_usd / &row.total_invested_usd * BigDecimal::from(100))
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
                        native_percent,
                        realized_usd: row.realized_usd.round(2).to_string(),
                        usd_percent,
                    },
                }
            })
            .collect();

        Ok(PnlLeaderboardResponse { ranks, total_count })
    }
}
