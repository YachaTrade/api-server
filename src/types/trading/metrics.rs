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

    pub fn to_chart_interval(&self) -> &'static str {
        match self {
            TimeFrame::OneMinute => "1",
            TimeFrame::FiveMinutes => "5",
            TimeFrame::FifteenMinutes => "15",
            TimeFrame::ThirtyMinutes => "30",
            TimeFrame::OneHour => "1H",
            TimeFrame::FourHours => "4H",
            TimeFrame::OneDay => "D",
            TimeFrame::OneWeek => "W",
            TimeFrame::OneMonth => "M",
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
    pub price_change_percent: String,
    pub current_price: Option<String>,
    pub start_price: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenTradingMetricsBatch {
    pub token_id: String,
    pub metrics: Vec<TokenTradingMetrics>,
}

#[derive(Clone)]
pub struct MetricsController {
    pub db: Arc<PostgresDatabase>,
}

impl MetricsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetricsController { db }
    }

    /// Get price data from chart table for price change calculation
    async fn get_price_data(
        &self,
        token_id: &str,
        timeframe: &TimeFrame,
    ) -> Result<(Option<String>, Option<String>)> {
        // Try to get from chart table first
        if let Ok((start_price, current_price)) =
            self.get_price_from_chart(token_id, timeframe).await
        {
            if start_price.is_some() && current_price.is_some() {
                return Ok((start_price, current_price));
            }
        }

        // If chart data is not available, return None for both prices
        // This will result in 0% price change
        Ok((None, None))
    }

    /// Get price data from chart table
    async fn get_price_from_chart(
        &self,
        token_id: &str,
        timeframe: &TimeFrame,
    ) -> Result<(Option<String>, Option<String>)> {
        let interval_type = timeframe.to_chart_interval();
        let period_seconds = timeframe.to_seconds();
        let timeframe_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - period_seconds;

        // Get both start and current prices in a single query using window functions
        let combined_query = sqlx::query!(
            r#"
            WITH price_data AS (
                SELECT 
                    open_price,
                    close_price,
                    time_stamp,
                    ROW_NUMBER() OVER (ORDER BY time_stamp ASC) as first_row,
                    ROW_NUMBER() OVER (ORDER BY time_stamp DESC) as last_row
                FROM chart 
                WHERE token_id = $1 
                AND interval_type = $2
                AND time_stamp >= $3
            )
            SELECT 
                FIRST_VALUE(open_price) OVER (ORDER BY first_row) as start_price,
                FIRST_VALUE(close_price) OVER (ORDER BY last_row) as current_price
            FROM price_data 
            WHERE first_row = 1 OR last_row = 1
            LIMIT 1
            "#,
            token_id,
            interval_type,
            timeframe_ago
        );

        let result = combined_query
            .fetch_optional(self.db.get_read_pool())
            .await?;

        let (start_price, current_price) = match result {
            Some(row) => (
                row.start_price.map(|p| p.to_plain_string()),
                row.current_price.map(|p| p.to_plain_string()),
            ),
            None => (None, None),
        };

        Ok((start_price, current_price))
    }

    /// Calculate price change percentage
    fn calculate_price_change_percent(start_price: &str, current_price: &str) -> Option<String> {
        let start: f64 = start_price.parse().ok()?;
        let current: f64 = current_price.parse().ok()?;

        if start == 0.0 {
            return None;
        }

        let change_percent = ((current - start) / start) * 100.0;
        Some(format!("{:.2}", change_percent))
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

        // Get price data for price change calculation
        let (start_price, current_price) = self
            .get_price_data(token_id, &timeframe)
            .await
            .unwrap_or((None, None));

        // Calculate price change percentage
        let price_change_percent = match (&start_price, &current_price) {
            (Some(start), Some(current)) => {
                Self::calculate_price_change_percent(start, current).unwrap_or("0.00".to_string())
            }
            _ => "0.00".to_string(), // Default to 0% when chart data is unavailable
        };

        Ok(TokenTradingMetrics {
            token_id: token_id.to_string(),
            buy_count: result.buy_count.unwrap_or(0),
            sell_count: result.sell_count.unwrap_or(0),
            volume: result.volume.unwrap_or(0.into()).to_string(),
            timeframe: timeframe.to_display_string().to_string(),
            price_change_percent,
            current_price,
            start_price,
        })
    }

    /// Get trading metrics for multiple timeframes concurrently
    pub async fn trading_metrics_batch(
        &self,
        token_id: &str,
        timeframes: Vec<TimeFrame>,
    ) -> Result<TokenTradingMetricsBatch> {
        if timeframes.is_empty() {
            return Ok(TokenTradingMetricsBatch {
                token_id: token_id.to_string(),
                metrics: vec![],
            });
        }

        // Create tasks for concurrent execution
        let mut tasks = Vec::new();
        for timeframe in timeframes {
            let controller = self.clone();
            let token_id_clone = token_id.to_string();
            let task = tokio::spawn(async move {
                controller.trading_metrics(&token_id_clone, timeframe).await
            });
            tasks.push(task);
        }

        // Collect all results
        let mut metrics_results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(metrics)) => metrics_results.push(metrics),
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(anyhow::anyhow!("Task execution failed")),
            }
        }

        Ok(TokenTradingMetricsBatch {
            token_id: token_id.to_string(),
            metrics: metrics_results,
        })
    }
}
