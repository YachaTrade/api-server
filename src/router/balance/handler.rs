use axum::{
    extract::{Path as AxumPath, State},
    Json,
};

use crate::{
    db::postgres::controller::balance::BalanceController,
    result::{AppError, AppJsonResult},
    state::AppState,
    types::response::HoldTokenResponse,
};

use super::path::Path;

/// Get balance of the account
#[utoipa::path(
    get,
    path = Path::GetBalance.docs_str(),
    params(
        ("account_address" = String, Path, description = "Account address")
    ),
    responses(
        (status = 200, description = "Balance fetched successfully", body = Vec<HoldTokenResponse>),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Balance"
)]

pub async fn get_balance(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>, // account_id를 Path 파라미터로 받음
) -> AppJsonResult<Vec<HoldTokenResponse>> {
    let balance_controller = BalanceController::new(state.postgres.clone());
    let balances = balance_controller
        .get_balances(&account_id)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(balances))
}
