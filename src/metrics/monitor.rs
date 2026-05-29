use std::time::Duration;
use tokio::time::interval;
use tracing::info;

use crate::config::METRICS_REPORT_INTERVAL;

pub async fn metrics_logging_task() -> anyhow::Result<()> {
    let interval_ms = *METRICS_REPORT_INTERVAL;
    info!("[METRICS] logging started with {}ms interval", interval_ms);

    let mut ticker = interval(Duration::from_millis(interval_ms));

    loop {
        ticker.tick().await;
        log_metrics_snapshot().await;
    }
}

async fn log_metrics_snapshot() {
    let (pg_timeouts, redis_timeouts, pg_avg_time, redis_avg_time) =
        crate::metrics::METRICS.db.get_values();

    info!("[METRICS] === METRICS REPORT ===");
    info!("[METRICS] PostgreSQL Timeouts: {}", pg_timeouts);
    info!("[METRICS] Redis Timeouts: {}", redis_timeouts);
    info!("[METRICS] PostgreSQL Avg Query Time: {:.2}ms", pg_avg_time);
    info!("[METRICS] Redis Avg Query Time: {:.2}ms", redis_avg_time);

    let (
        gift_events,
        gift_sig_failures,
        gift_crc,
        gift_ingested,
        gift_tx_success,
        gift_tx_failure,
        gift_reply_success,
        gift_reply_failure,
        gift_db_read_errors,
    ) = crate::metrics::METRICS.gift.get_values();
    info!(
        "[METRICS] Gift: webhook_events={} signature_failures={} crc={} ingested={} tx_success={} tx_failure={} reply_success={} reply_failure={} db_read_errors={}",
        gift_events,
        gift_sig_failures,
        gift_crc,
        gift_ingested,
        gift_tx_success,
        gift_tx_failure,
        gift_reply_success,
        gift_reply_failure,
        gift_db_read_errors,
    );
    info!("[METRICS] =======================");
}
