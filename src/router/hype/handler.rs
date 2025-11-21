use axum::{
    Extension, Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::HypePath;
use crate::{
    result::AppJsonResult,
    services::hype::HypeService,
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        hype::{
            AmountResponse, HypeEpochResponse, HypePointRecordResponse, HypePointResponse,
            HypeRewardAddHistoryResponse, HypeRewardHistoryResponse, HypeTokenQuery,
            HypeTokenResponse, HypeVoteHistoryResponse, HypeVoteRequest, HypeVoteResponse,
        },
    },
};

/// Get Hype Token
#[utoipa::path(
    get,
    path = HypePath::GetHype.docs_str(),
    params(
        ("epoch" = Option<i64>, Query, description = "Optional epoch parameter")
    ),
    responses(
        (status = 200, description = "Hype Token fetched successfully", body = HypeTokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_hype_token(
    State(state): State<AppState>,
    Query(query): Query<HypeTokenQuery>,
) -> AppJsonResult<HypeTokenResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_hype_token(query).await?;

    Ok(Json(response))
}

/// Get Latest Hype Token (ACTIVE or most recent COMPLETE)
#[utoipa::path(
    get,
    path = HypePath::GetHypeLatest.docs_str(),
    responses(
        (status = 200, description = "Latest Hype Token fetched successfully", body = HypeTokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_hype_token_latest(
    State(state): State<AppState>,
) -> AppJsonResult<HypeTokenResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_hype_token_latest().await?;

    Ok(Json(response))
}

/// Get Hype Point
#[utoipa::path(
    get,
    path = HypePath::GetPoint.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype point fetched successfully", body = HypePointResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_point(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<HypePointResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_hype_point(&session_address).await?;

    Ok(Json(response))
}

/// Get Hype Epoch
#[utoipa::path(
    get,
    path = HypePath::GetEpoch.docs_str(),
    responses(
        (status = 200, description = "Hype epoch fetched successfully", body = HypeEpochResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_epoch(State(state): State<AppState>) -> AppJsonResult<HypeEpochResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_hype_epoch().await?;

    Ok(Json(response))
}

/// Get Hype Vote History
#[utoipa::path(
    get,
    path = HypePath::GetVoteHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype vote history fetched successfully", body = HypeVoteHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_vote_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeVoteHistoryResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_hype_vote_history(&session_address, &params)
        .await?;

    Ok(Json(response))
}

/// Get Hype Point History
#[utoipa::path(
    get,
    path = HypePath::GetPointHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype point history fetched successfully", body = HypePointRecordResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_point_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypePointRecordResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_hype_point_history(&session_address, &params)
        .await?;

    Ok(Json(response))
}

/// Get Hype Reward History
#[utoipa::path(
    get,
    path = HypePath::GetRewardHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype reward history fetched successfully", body = HypeRewardHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_reward_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeRewardHistoryResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_hype_reward_history(&session_address, &params)
        .await?;

    Ok(Json(response))
}

/// Get Hype Reward Add History
#[utoipa::path(
    get,
    path = HypePath::GetRewardAddHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype reward add history fetched successfully", body = HypeRewardAddHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_reward_add_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeRewardAddHistoryResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_hype_reward_add_history(&session_address, &params)
        .await?;

    Ok(Json(response))
}

/// Vote for Hype Token
#[utoipa::path(
    post,
    path = HypePath::Vote.docs_str(),
    request_body = HypeVoteRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Vote submitted successfully", body = HypeVoteResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn vote(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<HypeVoteRequest>,
) -> AppJsonResult<HypeVoteResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.vote(&session_address, &payload).await?;

    Ok(Json(response))
}

/// Get Community Treasury
#[utoipa::path(
    get,
    path = HypePath::GetCommunityTreasury.docs_str(),
    responses(
        (status = 200, description = "Community treasury fetched successfully", body = AmountResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_community_treasury(
    State(state): State<AppState>,
) -> AppJsonResult<AmountResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_community_treasury().await?;

    Ok(Json(response))
}

/// Get Total Hype Point
#[utoipa::path(
    get,
    path = HypePath::GetTotalHypePoint.docs_str(),
    responses(
        (status = 200, description = "Total hype point fetched successfully", body = AmountResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_total_hype_point(State(state): State<AppState>) -> AppJsonResult<AmountResponse> {
    let service = HypeService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_total_hype_point().await?;

    Ok(Json(response))
}
