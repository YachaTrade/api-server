use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::trading::metrics::{
        MakerCount, MetricItem, MetricsBatchResponse, TimeFrame, TransactionCount, VolumeAmount,
    },
    utils::{
        calculate_price_change_percent, current_unix_timestamp,
        single_flight::{GLOBAL_CACHE, with_cache},
    },
};

pub struct MetricsController {
    pub db: Arc<PostgresDatabase>,
}

impl MetricsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetricsController { db }
    }

    pub async fn trading_metrics_batch(
        &self,
        token_id: &str,
        timeframes: Vec<TimeFrame>,
    ) -> Result<MetricsBatchResponse> {
        let mut handles = Vec::with_capacity(timeframes.len());

        for timeframe in timeframes {
            let token_id = token_id.to_string();
            let handle = tokio::spawn({
                let db = Arc::clone(&self.db);
                async move {
                    let controller = MetricsController::new(db);
                    controller
                        .fetch_metric_for_timeframe(&token_id, timeframe)
                        .await
                }
            });
            handles.push(handle);
        }

        let mut metrics = Vec::with_capacity(handles.len());
        for handle in handles {
            metrics.push(handle.await??);
        }

        // Sort metrics by timeframe to preserve order
        metrics.sort_by_key(|m| match m.timeframe.as_str() {
            "5m" => 0,
            "1h" => 1,
            "6h" => 2,
            "24h" => 3,
            _ => 999,
        });

        Ok(MetricsBatchResponse { metrics })
    }

    async fn fetch_metric_for_timeframe(
        &self,
        token_id: &str,
        timeframe: TimeFrame,
    ) -> Result<MetricItem> {
        let cache_key = cache_key!("metrics", token_id, timeframe.to_string());

        let token_id_clone = token_id.to_string();
        with_cache(&GLOBAL_CACHE.cache, &cache_key, || async move {
            self.fetch_metric_for_timeframe_internal(&token_id_clone, timeframe)
                .await
        })
        .await
    }

    async fn fetch_metric_for_timeframe_internal(
        &self,
        token_id: &str,
        timeframe: TimeFrame,
    ) -> Result<MetricItem> {
        let period_seconds = timeframe.to_seconds();
        let current_time = current_unix_timestamp();
        let timeframe_ago = current_time - period_seconds;

        #[derive(sqlx::FromRow)]
        struct MetricRow {
            buy_count: Option<i64>,
            sell_count: Option<i64>,
            buy_volume: Option<BigDecimal>,
            sell_volume: Option<BigDecimal>,
            buy_makers: Option<i64>,
            sell_makers: Option<i64>,
        }

        let swap_future = async {
            measure_postgres!(
                "trading_metrics.fetch_metric_for_timeframe",
                sqlx::query_as::<_, MetricRow>(
                    r#"
                    SELECT
                        SUM(CASE WHEN is_buy = true THEN 1 ELSE 0 END) as buy_count,
                        SUM(CASE WHEN is_buy = false THEN 1 ELSE 0 END) as sell_count,
                        COALESCE(SUM(CASE WHEN is_buy = true THEN value ELSE 0 END), 0) as buy_volume,
                        COALESCE(SUM(CASE WHEN is_buy = false THEN value ELSE 0 END), 0) as sell_volume,
                        COUNT(DISTINCT CASE WHEN is_buy = true THEN account_id END) as buy_makers,
                        COUNT(DISTINCT CASE WHEN is_buy = false THEN account_id END) as sell_makers
                    FROM swap
                    WHERE token_id = $1
                    AND created_at > $2
                    AND created_at <= $3
                    "#,
                )
                .bind(token_id)
                .bind(timeframe_ago)
                .bind(current_time)
                .fetch_one(self.db.get_read_pool())
            )
        };

        let price_future = self.get_price_from_chart(token_id, &timeframe);

        let (swap_result, price_result) = tokio::join!(swap_future, price_future);

        let row = swap_result?;
        let (start_price, current_price) = price_result?;

        let percent = match (start_price, current_price) {
            (Some(start), Some(current)) => {
                calculate_price_change_percent(&start, &current).unwrap_or(0.0)
            }

            _ => 0.0,
        };

        let buy_count = row.buy_count.unwrap_or(0);
        let sell_count = row.sell_count.unwrap_or(0);
        let buy_volume = row.buy_volume.unwrap_or(BigDecimal::from(0));
        let sell_volume = row.sell_volume.unwrap_or(BigDecimal::from(0));
        let total_volume = &buy_volume + &sell_volume;
        let buy_makers = row.buy_makers.unwrap_or(0);
        let sell_makers = row.sell_makers.unwrap_or(0);

        Ok(MetricItem {
            timeframe: timeframe.to_string().to_string(),
            percent,
            transactions: TransactionCount {
                buy: buy_count,
                sell: sell_count,
                total: buy_count + sell_count,
            },
            volume: VolumeAmount {
                buy: buy_volume.normalized().to_plain_string(),
                sell: sell_volume.normalized().to_plain_string(),
                total: total_volume.normalized().to_plain_string(),
            },
            makers: MakerCount {
                buy: buy_makers,
                sell: sell_makers,
                total: buy_makers + sell_makers,
            },
        })
    }

    async fn get_price_from_chart(
        &self,
        token_id: &str,
        timeframe: &TimeFrame,
    ) -> Result<(Option<String>, Option<String>)> {
        let period_seconds = timeframe.to_seconds();
        let current_time = current_unix_timestamp();
        let timeframe_ago = current_time - period_seconds;

        // Get start price: most recent price before or at timeframe_ago, or oldest price if not available
        let start_result = measure_postgres!(
            "trading_metrics.get_start_price",
            sqlx::query!(
                r#"
                SELECT COALESCE(
                    (SELECT price
                     FROM price_history
                     WHERE token_id = $1 AND created_at <= $2
                     ORDER BY created_at DESC, block_number DESC, tx_index DESC, log_index DESC
                     LIMIT 1),
                    (SELECT price
                     FROM price_history
                     WHERE token_id = $1
                     ORDER BY created_at ASC, block_number ASC, tx_index ASC, log_index ASC
                     LIMIT 1)
                ) as price
                "#,
                token_id,
                timeframe_ago
            )
            .fetch_optional(self.db.get_read_pool())
        )?;

        // Get current price: most recent price
        let current_result = measure_postgres!(
            "trading_metrics.get_current_price",
            sqlx::query!(
                r#"
                SELECT price
                FROM price_history
                WHERE token_id = $1
                ORDER BY created_at DESC, block_number DESC, tx_index DESC, log_index DESC
                LIMIT 1
                "#,
                token_id
            )
            .fetch_optional(self.db.get_read_pool())
        )?;

        let start_price = start_result
            .and_then(|row| row.price)
            .map(|price| price.normalized().to_plain_string());

        let current_price = current_result.map(|row| row.price.normalized().to_plain_string());

        Ok((start_price, current_price))
    }
}
