use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::metrics::METRICS;

pub async fn get_metrics() -> Response {
    let metrics_output = METRICS.export_prometheus_metrics();

    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics_output,
    )
        .into_response()
}
