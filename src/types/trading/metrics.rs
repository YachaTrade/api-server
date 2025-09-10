use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TimeFrame {
    #[serde(rename = "1")]
    OneMinute,
    #[serde(rename = "5")]
    FiveMinutes,
    #[serde(rename = "15")]
    FifteenMinutes,
    #[serde(rename = "30")]
    ThirtyMinutes,
    #[serde(rename = "60")]
    OneHour,
    #[serde(rename = "4H")]
    FourHours,
    #[serde(rename = "D")]
    OneDay,
    #[serde(rename = "W")]
    OneWeek,
    #[serde(rename = "M")]
    OneMonth,
}

impl TimeFrame {
    pub fn to_seconds(&self) -> i64 {
        match self {
            TimeFrame::OneMinute => 60,
            TimeFrame::FiveMinutes => 300,
            TimeFrame::FifteenMinutes => 900,
            TimeFrame::ThirtyMinutes => 1800,
            TimeFrame::OneHour => 3600,
            TimeFrame::FourHours => 14400,
            TimeFrame::OneDay => 86400,
            TimeFrame::OneWeek => 604800,
            TimeFrame::OneMonth => 2592000, // 30 days
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            TimeFrame::OneMinute => "1m",
            TimeFrame::FiveMinutes => "5m",
            TimeFrame::FifteenMinutes => "15m",
            TimeFrame::ThirtyMinutes => "30m",
            TimeFrame::OneHour => "1h",
            TimeFrame::FourHours => "4h",
            TimeFrame::OneDay => "1d",
            TimeFrame::OneWeek => "1w",
            TimeFrame::OneMonth => "1M",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenTradingMetrics {
    pub token_id: String,
    pub buy_count: i64,
    pub sell_count: i64,
    pub volume: String,
    pub timeframe: String,
}

pub struct MetricsController {
    pub db: Arc<PostgresDatabase>,
}

impl MetricsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetricsController { db }
    }

    /// Get trading metrics for a specific token and timeframe
    pub async fn trading_metrics(
        &self,
        token_id: &str,
        timeframe: TimeFrame,
    ) -> Result<TokenTradingMetrics> {
        let start_time = Instant::now();

        // Calculate timestamp for the specified timeframe ago
        let period_seconds = timeframe.to_seconds();
        let timeframe_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - period_seconds;

        let query = sqlx::query!(
            r#"
            SELECT 
                SUM(CASE WHEN is_buy = true THEN 1 ELSE 0 END) as buy_count,
                SUM(CASE WHEN is_buy = false THEN 1 ELSE 0 END) as sell_count,
                COALESCE(SUM(native_amount), 0) as volume
            FROM swap 
            WHERE token_id = $1 
            AND created_at > $2
            "#,
            token_id,
            timeframe_ago
        );

        let result = tokio::time::timeout(
            Duration::from_millis(1000),
            query.fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Query timeout after 1000ms"))?
        .map_err(|err| anyhow::anyhow!("Failed to get trading metrics. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        info!(
            "trading_metrics(token_id: {}, timeframe: {}) completed in {:?}",
            token_id,
            timeframe.to_display_string(),
            elapsed
        );

        if elapsed > Duration::from_millis(100) {
            warn!(
                "trading_metrics query slow performance: {:?} for token_id: {}, timeframe: {}",
                elapsed,
                token_id,
                timeframe.to_display_string()
            );
        }

        Ok(TokenTradingMetrics {
            token_id: token_id.to_string(),
            buy_count: result.buy_count.unwrap_or(0),
            sell_count: result.sell_count.unwrap_or(0),
            volume: result.volume.unwrap_or(0.into()).to_string(),
            timeframe: timeframe.to_display_string().to_string(),
        })
    }
}
