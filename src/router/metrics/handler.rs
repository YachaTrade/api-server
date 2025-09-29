use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::metrics::METRICS;

pub async fn get_metrics() -> Response {
    let mut output = String::new();
    let (pg_timeouts, redis_timeouts, pg_avg_time, redis_avg_time) = METRICS.db.get_values();
    output.push_str(&format!("db_postgres_timeouts_total {}\n", pg_timeouts));
    output.push_str(&format!("db_redis_timeouts_total {}\n", redis_timeouts));
    output.push_str(&format!(
        "db_postgres_avg_query_time_ms {:.2}\n",
        pg_avg_time
    ));
    output.push_str(&format!(
        "db_redis_avg_query_time_ms {:.2}\n",
        redis_avg_time
    ));

    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
        output,
    )
        .into_response()
}
