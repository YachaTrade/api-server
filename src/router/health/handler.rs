use axum::{
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::metrics::METRICS;

/// Health check endpoint for ALB target group
///
/// Returns:
/// - 200 OK: Service is healthy (avg response times within thresholds)
/// - 503 SERVICE_UNAVAILABLE: Service is unhealthy (database performance degraded)
pub async fn health_check() -> impl IntoResponse {
    let (pg_timeouts, redis_timeouts, pg_avg_time, redis_avg_time) = METRICS.db.get_values();

    // Thresholds:
    // - PostgreSQL: 1500ms (actual timeout is 2000ms)
    // - Redis: 400ms (actual timeout is 300ms, but avg can exceed if some succeed)
    const PG_THRESHOLD_MS: f64 = 1500.0;
    const REDIS_THRESHOLD_MS: f64 = 400.0;

    if pg_avg_time > PG_THRESHOLD_MS || redis_avg_time > REDIS_THRESHOLD_MS {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "unhealthy",
                "reason": "high_latency",
                "metrics": {
                    "postgres_avg_ms": pg_avg_time,
                    "redis_avg_ms": redis_avg_time,
                    "postgres_timeouts": pg_timeouts,
                    "redis_timeouts": redis_timeouts
                },
                "thresholds": {
                    "postgres_ms": PG_THRESHOLD_MS,
                    "redis_ms": REDIS_THRESHOLD_MS
                }
            }))
        ).into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "metrics": {
                "postgres_avg_ms": pg_avg_time,
                "redis_avg_ms": redis_avg_time,
                "postgres_timeouts": pg_timeouts,
                "redis_timeouts": redis_timeouts
            }
        }))
    ).into_response()
}
