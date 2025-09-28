use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DbMetrics {
    pub query_count: Arc<AtomicU64>,
    pub timeout_count: Arc<AtomicU64>,
    pub success_count: Arc<AtomicU64>,
    pub total_duration_ms: Arc<AtomicU64>,
}

impl DbMetrics {
    pub fn new() -> Self {
        Self {
            query_count: Arc::new(AtomicU64::new(0)),
            timeout_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            total_duration_ms: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[derive(Debug)]
pub struct MetricsManager {
    // PostgreSQL metrics by operation
    pub postgres_operations: Arc<RwLock<HashMap<String, DbMetrics>>>,

    // Redis metrics by operation
    pub redis_operations: Arc<RwLock<HashMap<String, DbMetrics>>>,

    // Global counters
    pub total_postgres_timeouts: Arc<AtomicU64>,
    pub total_redis_timeouts: Arc<AtomicU64>,
    pub total_postgres_queries: Arc<AtomicU64>,
    pub total_redis_queries: Arc<AtomicU64>,
}

impl MetricsManager {
    pub fn new() -> Self {
        Self {
            postgres_operations: Arc::new(RwLock::new(HashMap::new())),
            redis_operations: Arc::new(RwLock::new(HashMap::new())),
            total_postgres_timeouts: Arc::new(AtomicU64::new(0)),
            total_redis_timeouts: Arc::new(AtomicU64::new(0)),
            total_postgres_queries: Arc::new(AtomicU64::new(0)),
            total_redis_queries: Arc::new(AtomicU64::new(0)),
        }
    }

    // PostgreSQL metrics
    pub fn get_or_create_postgres_metrics(&self, operation: &str) -> DbMetrics {
        let mut postgres_ops = self.postgres_operations.write().unwrap();
        postgres_ops
            .entry(operation.to_string())
            .or_insert_with(DbMetrics::new)
            .clone()
    }

    pub fn record_postgres_timeout(&self, operation: &str) {
        let metrics = self.get_or_create_postgres_metrics(operation);
        metrics.timeout_count.fetch_add(1, Ordering::Relaxed);
        self.total_postgres_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_postgres_success(&self, operation: &str, duration: Duration) {
        let metrics = self.get_or_create_postgres_metrics(operation);
        metrics.query_count.fetch_add(1, Ordering::Relaxed);
        metrics.success_count.fetch_add(1, Ordering::Relaxed);
        metrics
            .total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.total_postgres_queries.fetch_add(1, Ordering::Relaxed);
    }

    // Redis metrics
    pub fn get_or_create_redis_metrics(&self, operation: &str) -> DbMetrics {
        let mut redis_ops = self.redis_operations.write().unwrap();
        redis_ops
            .entry(operation.to_string())
            .or_insert_with(DbMetrics::new)
            .clone()
    }

    pub fn record_redis_timeout(&self, operation: &str) {
        let metrics = self.get_or_create_redis_metrics(operation);
        metrics.timeout_count.fetch_add(1, Ordering::Relaxed);
        self.total_redis_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_redis_success(&self, operation: &str, duration: Duration) {
        let metrics = self.get_or_create_redis_metrics(operation);
        metrics.query_count.fetch_add(1, Ordering::Relaxed);
        metrics.success_count.fetch_add(1, Ordering::Relaxed);
        metrics
            .total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.total_redis_queries.fetch_add(1, Ordering::Relaxed);
    }

    // Generic method to record DB operations
    pub fn record_db_timeout(&self, db_type: &str, operation: &str) {
        match db_type {
            "postgres" => self.record_postgres_timeout(operation),
            "redis" => self.record_redis_timeout(operation),
            _ => {}
        }
    }

    pub fn record_db_duration(&self, db_type: &str, operation: &str, duration: Duration) {
        match db_type {
            "postgres" => self.record_postgres_success(operation, duration),
            "redis" => self.record_redis_success(operation, duration),
            _ => {}
        }
    }

    // Export metrics for Prometheus
    pub fn export_prometheus_metrics(&self) -> String {
        let mut output = String::new();

        // Global counters
        output.push_str(&format!(
            "# HELP postgres_timeouts_total Total number of PostgreSQL timeouts\n\
             # TYPE postgres_timeouts_total counter\n\
             postgres_timeouts_total {}\n\n",
            self.total_postgres_timeouts.load(Ordering::Relaxed)
        ));

        output.push_str(&format!(
            "# HELP redis_timeouts_total Total number of Redis timeouts\n\
             # TYPE redis_timeouts_total counter\n\
             redis_timeouts_total {}\n\n",
            self.total_redis_timeouts.load(Ordering::Relaxed)
        ));

        output.push_str(&format!(
            "# HELP postgres_queries_total Total number of PostgreSQL queries\n\
             # TYPE postgres_queries_total counter\n\
             postgres_queries_total {}\n\n",
            self.total_postgres_queries.load(Ordering::Relaxed)
        ));

        output.push_str(&format!(
            "# HELP redis_queries_total Total number of Redis queries\n\
             # TYPE redis_queries_total counter\n\
             redis_queries_total {}\n\n",
            self.total_redis_queries.load(Ordering::Relaxed)
        ));

        // PostgreSQL operation metrics
        if let Ok(postgres_ops) = self.postgres_operations.read() {
            for (operation, metrics) in postgres_ops.iter() {
                output.push_str(&format!(
                    "# HELP postgres_operation_queries_total Total queries by operation\n\
                     # TYPE postgres_operation_queries_total counter\n\
                     postgres_operation_queries_total{{operation=\"{}\"}} {}\n\n",
                    operation,
                    metrics.query_count.load(Ordering::Relaxed)
                ));

                output.push_str(&format!(
                    "# HELP postgres_operation_timeouts_total Total timeouts by operation\n\
                     # TYPE postgres_operation_timeouts_total counter\n\
                     postgres_operation_timeouts_total{{operation=\"{}\"}} {}\n\n",
                    operation,
                    metrics.timeout_count.load(Ordering::Relaxed)
                ));

                let total_queries = metrics.query_count.load(Ordering::Relaxed);
                let avg_duration = if total_queries > 0 {
                    metrics.total_duration_ms.load(Ordering::Relaxed) as f64 / total_queries as f64
                } else {
                    0.0
                };

                output.push_str(&format!(
                    "# HELP postgres_operation_duration_avg_ms Average query duration by operation\n\
                     # TYPE postgres_operation_duration_avg_ms gauge\n\
                     postgres_operation_duration_avg_ms{{operation=\"{}\"}} {:.2}\n\n",
                    operation,
                    avg_duration
                ));
            }
        }

        // Redis operation metrics
        if let Ok(redis_ops) = self.redis_operations.read() {
            for (operation, metrics) in redis_ops.iter() {
                output.push_str(&format!(
                    "# HELP redis_operation_queries_total Total queries by operation\n\
                     # TYPE redis_operation_queries_total counter\n\
                     redis_operation_queries_total{{operation=\"{}\"}} {}\n\n",
                    operation,
                    metrics.query_count.load(Ordering::Relaxed)
                ));

                output.push_str(&format!(
                    "# HELP redis_operation_timeouts_total Total timeouts by operation\n\
                     # TYPE redis_operation_timeouts_total counter\n\
                     redis_operation_timeouts_total{{operation=\"{}\"}} {}\n\n",
                    operation,
                    metrics.timeout_count.load(Ordering::Relaxed)
                ));

                let total_queries = metrics.query_count.load(Ordering::Relaxed);
                let avg_duration = if total_queries > 0 {
                    metrics.total_duration_ms.load(Ordering::Relaxed) as f64 / total_queries as f64
                } else {
                    0.0
                };

                output.push_str(&format!(
                    "# HELP redis_operation_duration_avg_ms Average query duration by operation\n\
                     # TYPE redis_operation_duration_avg_ms gauge\n\
                     redis_operation_duration_avg_ms{{operation=\"{}\"}} {:.2}\n\n",
                    operation, avg_duration
                ));
            }
        }

        output
    }
}

lazy_static! {
    pub static ref METRICS: MetricsManager = MetricsManager::new();
}
