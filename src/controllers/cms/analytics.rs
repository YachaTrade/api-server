use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::cms::analytics::{
        ChesterRetentionResponse, ChesterRetentionRound, NewUsersResponse,
        TopHeldToken, UserActivityResponse, UserRoiResponse,
    },
};

pub struct AnalyticsController {
    db: Arc<PostgresDatabase>,
}

#[derive(Debug, sqlx::FromRow)]
struct UserActivityStatsRow {
    total_users: i64,
    avg_pnl_usd: Option<BigDecimal>,
    avg_pnl_native: Option<BigDecimal>,
    avg_volume_usd: Option<BigDecimal>,
    avg_volume_native: Option<BigDecimal>,
}

#[derive(Debug, sqlx::FromRow)]
struct TopHeldTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    holder_count: i64,
    avg_balance: BigDecimal,
}

#[derive(Debug, sqlx::FromRow)]
struct RoiStatsRow {
    avg_roi_percent: Option<BigDecimal>,
    positive_roi_count: i64,
    negative_roi_count: i64,
    total_users: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ChesterRetentionRow {
    from_round: i64,
    to_round: i64,
    from_participants: i64,
    returning_participants: i64,
}

impl AnalyticsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// 이탈 또는 활성 유저 분석
    /// is_churned=true: last_swap_at < cutoff (이탈)
    /// is_churned=false: last_swap_at >= cutoff (활성)
    pub async fn get_user_activity(
        &self,
        inactive_days: i64,
        limit: i64,
        is_churned: bool,
    ) -> Result<UserActivityResponse> {
        let cutoff_seconds = inactive_days * 86400;

        // 1. 유저 수 + 평균 PnL + 평균 볼륨
        let comparison = if is_churned { "<" } else { ">=" };
        let stats_query = format!(
            r#"
            WITH target_users AS (
                SELECT aa.account_id
                FROM account_activity aa
                WHERE (EXTRACT(EPOCH FROM NOW()) - aa.last_swap_at) {} $1
            )
            SELECT
                COUNT(DISTINCT tu.account_id) as total_users,
                AVG(pa.realized_usd + pa.unrealized_usd) as avg_pnl_usd,
                AVG(pa.realized_native + pa.unrealized_native) as avg_pnl_native,
                AVG(vol.volume_usd) as avg_volume_usd,
                AVG(vol.volume_native) as avg_volume_native
            FROM target_users tu
            LEFT JOIN pnl_aggregator pa ON tu.account_id = pa.account_id
            LEFT JOIN (
                SELECT account_id, SUM(value) as volume_usd, SUM(native_amount) as volume_native
                FROM swap
                GROUP BY account_id
            ) vol ON tu.account_id = vol.account_id
            "#,
            comparison
        );

        let stats = measure_postgres!(
            "analytics.user_activity_stats",
            sqlx::query_as::<_, UserActivityStatsRow>(&stats_query)
                .bind(cutoff_seconds)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch user activity stats: {}", err))?;

        // 2. Top 보유 토큰
        let tokens_query = format!(
            r#"
            WITH target_users AS (
                SELECT aa.account_id
                FROM account_activity aa
                WHERE (EXTRACT(EPOCH FROM NOW()) - aa.last_swap_at) {} $1
            )
            SELECT
                b.token_id,
                COALESCE(t.name, '') as name,
                COALESCE(t.symbol, '') as symbol,
                COUNT(DISTINCT b.account_id) as holder_count,
                AVG(b.balance) as avg_balance
            FROM balance b
            INNER JOIN target_users tu ON b.account_id = tu.account_id
            LEFT JOIN token t ON b.token_id = t.token_id
            WHERE b.balance > 0
            GROUP BY b.token_id, t.name, t.symbol
            ORDER BY holder_count DESC
            LIMIT $2
            "#,
            comparison
        );

        let token_rows = measure_postgres!(
            "analytics.top_held_tokens",
            sqlx::query_as::<_, TopHeldTokenRow>(&tokens_query)
                .bind(cutoff_seconds)
                .bind(limit)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch top held tokens: {}", err))?;

        let top_held_tokens = token_rows
            .into_iter()
            .map(|row| TopHeldToken {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                holder_count: row.holder_count,
                avg_balance: row.avg_balance.round(2).to_string(),
            })
            .collect();

        Ok(UserActivityResponse {
            total_users: stats.total_users,
            avg_pnl_usd: stats.avg_pnl_usd.unwrap_or_default().round(2).to_string(),
            avg_pnl_native: stats.avg_pnl_native.unwrap_or_default().round(2).to_string(),
            avg_volume_usd: stats.avg_volume_usd.unwrap_or_default().round(2).to_string(),
            avg_volume_native: stats.avg_volume_native.unwrap_or_default().round(2).to_string(),
            top_held_tokens,
        })
    }

    /// 신규 유저 수 (첫 swap 기준)
    pub async fn get_new_users(&self, days: i64) -> Result<NewUsersResponse> {
        let cutoff_seconds = days * 86400;

        let count = measure_postgres!(
            "analytics.new_users",
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*) as count
                FROM account_activity
                WHERE (EXTRACT(EPOCH FROM NOW()) - first_swap_at) <= $1
                "#
            )
            .bind(cutoff_seconds)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch new users count: {}", err))?;

        Ok(NewUsersResponse {
            new_users: count,
            period_days: days,
        })
    }

    /// 유저별 ROI 통계
    pub async fn get_user_roi(&self) -> Result<UserRoiResponse> {
        let stats = measure_postgres!(
            "analytics.user_roi_stats",
            sqlx::query_as::<_, RoiStatsRow>(
                r#"
                SELECT
                    AVG(
                        CASE WHEN total_invested_usd > 0
                        THEN ((realized_usd + unrealized_usd) / total_invested_usd) * 100
                        ELSE 0 END
                    ) as avg_roi_percent,
                    COUNT(CASE WHEN (realized_usd + unrealized_usd) > 0 THEN 1 END) as positive_roi_count,
                    COUNT(CASE WHEN (realized_usd + unrealized_usd) <= 0 THEN 1 END) as negative_roi_count,
                    COUNT(*) as total_users
                FROM pnl_aggregator
                WHERE total_invested_usd > 0
                "#
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch ROI stats: {}", err))?;

        // 중앙값 계산
        let median = measure_postgres!(
            "analytics.user_roi_median",
            sqlx::query_scalar::<_, BigDecimal>(
                r#"
                SELECT PERCENTILE_CONT(0.5) WITHIN GROUP (
                    ORDER BY ((realized_usd + unrealized_usd) / total_invested_usd) * 100
                )::NUMERIC as median_roi
                FROM pnl_aggregator
                WHERE total_invested_usd > 0
                "#
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch ROI median: {}", err))?;

        Ok(UserRoiResponse {
            avg_roi_percent: stats.avg_roi_percent.unwrap_or_default().round(2).to_string(),
            median_roi_percent: median.round(2).to_string(),
            positive_roi_count: stats.positive_roi_count,
            negative_roi_count: stats.negative_roi_count,
            total_users: stats.total_users,
        })
    }

    /// 체스터 라운드간 리텐션
    pub async fn get_chester_retention(&self) -> Result<ChesterRetentionResponse> {
        let rows = measure_postgres!(
            "analytics.chester_retention",
            sqlx::query_as::<_, ChesterRetentionRow>(
                r#"
                WITH round_participants AS (
                    SELECT DISTINCT round, account_id
                    FROM chester_box_reward
                ),
                round_pairs AS (
                    SELECT
                        r1.round as from_round,
                        r2.round as to_round,
                        COUNT(DISTINCT r1.account_id) as from_participants,
                        COUNT(DISTINCT CASE WHEN r2.account_id IS NOT NULL THEN r1.account_id END) as returning_participants
                    FROM round_participants r1
                    LEFT JOIN round_participants r2
                        ON r1.account_id = r2.account_id
                        AND r2.round = r1.round + 1
                    GROUP BY r1.round, r2.round
                )
                SELECT
                    from_round,
                    from_round + 1 as to_round,
                    from_participants,
                    returning_participants
                FROM round_pairs
                WHERE to_round IS NOT NULL OR from_round = (SELECT MAX(round) FROM chester_box_reward)
                ORDER BY from_round
                "#
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch chester retention: {}", err))?;

        let rounds = rows
            .into_iter()
            .map(|row| {
                let rate = if row.from_participants > 0 {
                    row.returning_participants as f64 / row.from_participants as f64 * 100.0
                } else {
                    0.0
                };
                ChesterRetentionRound {
                    from_round: row.from_round,
                    to_round: row.to_round,
                    from_participants: row.from_participants,
                    returning_participants: row.returning_participants,
                    retention_rate: format!("{:.2}", rate),
                }
            })
            .collect();

        Ok(ChesterRetentionResponse { rounds })
    }
}
