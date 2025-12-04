use axum::{Extension, Json, extract::{Query, State}};

use tracing::instrument;

use crate::{
    result::AppJsonResult,
    state::AppState,
    types::raffle::{RaffleCheckQuery, RaffleCheckResponse, RaffleStatusResponse},
    controllers::raffle::RaffleController,
};

use super::path::RafflePath;

/// Get raffle eligibility and entry count for authenticated user
#[utoipa::path(
    get,
    path = RafflePath::GetEligible.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Raffle eligibility status", body = RaffleStatusResponse)
    ),
    tag = "Raffle"
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

/// Check raffle entries and prizes for a specific round
#[utoipa::path(
    get,
    path = RafflePath::Check.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication"),
        ("round" = i64, Query, description = "Round number to check")
    ),
    responses(
        (status = 200, description = "Raffle entries and prize information", body = RaffleCheckResponse)
    ),
    tag = "Raffle"
)]
#[instrument(skip(state, session_address, query))]
pub async fn check_raffle(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(query): Query<RaffleCheckQuery>,
) -> AppJsonResult<RaffleCheckResponse> {
    let controller = RaffleController::new(state.postgres.clone());
    let response = controller.check_raffle(query.round, &session_address).await?;

    Ok(Json(response))
}
