use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{CountRow, info::AccountInfo},
        leaderboard::{HypePointLeaderboardEntry, HypePointLeaderboardResponse, LeaderboardQuery},
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
        let limit = query.limit.unwrap_or(10).min(100);
        let offset = query.offset.unwrap_or(0);

        let rows_future = async {
            measure_postgres!(
                "leaderboard.get_hype_point_leaderboard.rows",
                sqlx::query_as::<_, HypePointLeaderboardRow>(
                    r#"
                    SELECT
                        ROW_NUMBER() OVER (ORDER BY p.hype_point DESC) as rank,
                        a.account_id,
                        COALESCE(ax.x_handle, a.nickname) as nickname,
                        a.bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                        p.hype_point
                    FROM point p
                    JOIN account a ON p.account_id = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    WHERE p.hype_point > 0
                    ORDER BY p.hype_point DESC
                    LIMIT $1 OFFSET $2
                    "#,
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
            )
        };

        let count_future = async {
            measure_postgres!(
                "leaderboard.get_hype_point_leaderboard.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT total_count as count
                    FROM hype_point_leaderboard_count
                    WHERE id = 1
                    "#,
                )
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, count_result) = tokio::join!(rows_future, count_future);

        let rows = rows_result
            .map_err(|err| anyhow!("Failed to fetch hype point leaderboard: {}", err))?;
        let total_count = count_result
            .map_err(|err| anyhow!("Failed to fetch hype point leaderboard count: {}", err))?
            .count;

        let leaderboard = rows
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
            leaderboard,
            total_count,
        })
    }
}
