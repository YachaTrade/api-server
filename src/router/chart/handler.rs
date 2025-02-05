use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::trading::chart::{ChartController, ChartInterval, ChartQuery, ChartResponse},
};

use axum::{
    extract::{Path, Query, State},
    Json,
};

use tracing::instrument;

use super::path::ChartPath;

///Get Chart data
#[utoipa::path(
    get,
    path = ChartPath::GetChart.docs_str(),
    responses(
        (status = 200, description = "Success", body = ChartResponse),
        (status = 404, description = "Chart not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token" = String, Path, description = "Token ID"),
        ("interval" = String, Query, description = "Chart interval (1m, 5m, 15m, 30m, 1h, 4h, 1d, 1w)"),
        ("pagination" = Option<i16>, Query, description = "Page number (default 0, returns 300 records per page)")
    ),
    tag = "Chart",
)]
#[instrument(skip(state, token, query))]
pub async fn get_chart(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<ChartQuery>,
) -> AppJsonResult<ChartResponse> {
    let chart_interval = ChartInterval::from_str(&query.interval)
        .map_err(|err| AppError::BadRequest(format!("Invalid chart interval: {}", err)))?;

    let pagenation = query.pagination.unwrap_or(0);

    let chart_controller = ChartController::new(state.postgres.clone());

    let chart_response = chart_controller
        .get_chart(&token, chart_interval, pagenation)
        .await
        .map_err(|err| AppError::InternalError(format!("Failed to get chart: {}", err)))?;

    Ok(Json(chart_response))
}
