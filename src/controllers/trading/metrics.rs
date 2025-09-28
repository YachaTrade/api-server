use std::{sync::Arc, time::Instant};

use anyhow::Result;
use bigdecimal::BigDecimal;
use tracing::warn;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::trading::metrics::{TimeFrame, TokenTradingMetrics, TokenTradingMetricsBatch},
};

pub struct MetricsController {
    pub db: Arc<PostgresDatabase>,
}

impl MetricsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetricsController { db }
    }

    pub async fn trading_metrics(
        &self,
        token_id: &str,
        timeframe: TimeFrame,
    ) -> Result<TokenTradingMetrics> {
        let start_time = Instant::now();

        let period_seconds = timeframe.to_seconds();
        let timeframe_ago = current_unix_timestamp() - period_seconds;

        let row = measure_postgres!(
            "trading_metrics.trading_metrics",
            sqlx::query!(
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
            )
            .fetch_one(self.db.get_read_pool())
        )?;

        let elapsed = start_time.elapsed();
        if elapsed.as_millis() > 100 {
            warn!(
                "trading_metrics query slow performance: {:?} for token_id: {}, timeframe: {}",
                elapsed,
                token_id,
                timeframe.to_display_string()
            );
        }

        let (start_price, current_price) = self.get_price_from_chart(token_id, &timeframe).await?;

        let price_change_percent = match (&start_price, &current_price) {
            (Some(start), Some(current)) => {
                calculate_price_change_percent(start, current).unwrap_or_else(|| "0.00".to_string())
            }
            _ => "0.00".to_string(),
        };

        Ok(TokenTradingMetrics {
            token_id: token_id.to_string(),
            buy_count: row.buy_count.unwrap_or(0),
            sell_count: row.sell_count.unwrap_or(0),
            volume: row.volume.unwrap_or(BigDecimal::from(0)).to_string(),
            timeframe: timeframe.to_display_string().to_string(),
            price_change_percent,
            current_price,
            start_price,
        })
    }

    pub async fn trading_metrics_batch(
        &self,
        token_id: &str,
        timeframes: Vec<TimeFrame>,
    ) -> Result<TokenTradingMetricsBatch> {
        let mut metrics = Vec::with_capacity(timeframes.len());
        for timeframe in timeframes {
            metrics.push(self.trading_metrics(token_id, timeframe).await?);
        }

        Ok(TokenTradingMetricsBatch {
            token_id: token_id.to_string(),
            metrics,
        })
    }

    async fn get_price_from_chart(
        &self,
        token_id: &str,
        timeframe: &TimeFrame,
    ) -> Result<(Option<String>, Option<String>)> {
        let interval_type = timeframe.to_chart_interval();
        let period_seconds = timeframe.to_seconds();
        let timeframe_ago = current_unix_timestamp() - period_seconds;

        let result = measure_postgres!(
            "trading_metrics.get_price_from_chart",
            sqlx::query!(
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
            )
            .fetch_optional(self.db.get_read_pool())
        )?;

        let (start_price, current_price) = match result {
            Some(row) => (
                row.start_price.map(|p| p.to_plain_string()),
                row.current_price.map(|p| p.to_plain_string()),
            ),
            None => (None, None),
        };

        Ok((start_price, current_price))
    }
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn calculate_price_change_percent(start_price: &str, current_price: &str) -> Option<String> {
    let start: f64 = start_price.parse().ok()?;
    let current: f64 = current_price.parse().ok()?;

    if (start - 0.0).abs() < f64::EPSILON {
        return None;
    }

    let change_percent = ((current - start) / start) * 100.0;
    Some(format!("{:.2}", change_percent))
}
