use crate::{
    db::postgres::{
        controller::chart::ChartController,
        model::{Chart, ChartInterval},
    },
    result::{AppError, AppJsonResult},
    state::AppState,
};
use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ChartResponse {
    pub data: Vec<Chart>,
    pub token_id: String,
    pub interval: String,
    pub pagination: i16,
}

#[derive(Deserialize, ToSchema)]
pub struct ChartQuery {
    interval: String,
    pagination: Option<i16>,
}

#[utoipa::path(
    get,
    path = "/chart/{token}",
    responses(
        (status = 200, description = "Success", body = ChartResponse),
        (status = 404, description = "Chart not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token" = String, Path, description = "Token ID"),
        ("interval" = String, Query, description = "Chart interval (1m, 5m, 15m, 30m, 1h, 4h, 1d, 1w)"),
        ("pagination" = Option<i16>, Query, description = "Page number (0-based, returns 300 records per page)")
    ),
    tag = "Chart",
    operation_id="get chart data"
)]
#[instrument(skip(state, token, query))]
pub async fn get_chart(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<ChartQuery>,
) -> AppJsonResult<ChartResponse> {
    info!("Chart request for token: {}", token);
    let chart_interval = ChartInterval::from_str(&query.interval)
        .map_err(|err| AppError::BadRequest(format!("Invalid chart interval: {}", err)))?;

    let pagination = query.pagination.unwrap_or(1);

    let chart_controller = ChartController::new(state.postgres.clone());

    let chart = chart_controller
        .get_chart(&token, chart_interval, pagination)
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to get chart: {}", err)))?;
    let chart_response = ChartResponse {
        data: chart,
        token_id: token,
        interval: chart_interval.to_str().to_string(),
        pagination,
    };
    Ok(Json(chart_response))
}
