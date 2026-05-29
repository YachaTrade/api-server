use anyhow::Result;
use once_cell::sync::Lazy;
use tracing::warn;

pub mod db_metrics;
pub mod gift_metrics;
pub mod monitor;
pub mod query;

use db_metrics::DBMetrics;
use gift_metrics::GiftMetrics;

/// 중앙 집중화된 메트릭 관리
pub struct Metrics {
    pub db: DBMetrics,
    pub gift: GiftMetrics,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            db: DBMetrics::new(),
            gift: GiftMetrics::new(),
        }
    }
}

/// 전역 메트릭 인스턴스
pub static METRICS: Lazy<Metrics> = Lazy::new(Metrics::new);

pub async fn run_metrics_logging() -> Result<()> {
    match monitor::metrics_logging_task().await {
        Ok(()) => Ok(()),
        Err(err) => {
            warn!("[METRICS] logging task stopped: {err}");
            Err(err)
        }
    }
}
