use axum::{Extension, Json, extract::{Query, State}};

use serde::Deserialize;
use tracing::instrument;

use crate::{
    result::AppJsonResult,
    state::AppState,
    types::raffle::{PrizeListResponse, RaffleStatusResponse},
    controllers::raffle::RaffleController,
};

use super::path::RafflePath;

#[derive(Debug, Deserialize)]
pub struct PrizeQuery {
    pub round: i64,
}

/// Get raffle eligibility for authenticated user
#[utoipa::path(
    get,
    path = RafflePath::GetEligible.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get raffle eligibility successfully", body = RaffleStatusResponse)
    ),
    tag="Raffle"
)]
#[instrument(skip(state, session_address))]
pub async fn get_eligible(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<RaffleStatusResponse> {
    let controller = RaffleController::new(state.postgres.clone());
    let response = controller.get_raffle_status(&session_address).await?;

    Ok(Json(response))
}

/// Get prize list for a specific round
#[utoipa::path(
    get,
    path = RafflePath::GetPrizes.docs_str(),
    params(
        ("round" = i64, Query, description = "Round number")
    ),
    responses(
        (status = 200, description = "Get prize list successfully", body = PrizeListResponse)
    ),
    tag="Raffle"
)]
#[instrument(skip(state, query))]
pub async fn get_prizes(
    State(state): State<AppState>,
    Query(query): Query<PrizeQuery>,
) -> AppJsonResult<PrizeListResponse> {
    let controller = RaffleController::new(state.postgres.clone());
    let response = controller.get_prize_list(query.round).await?;

    Ok(Json(response))
}
