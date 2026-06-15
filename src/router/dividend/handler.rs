use axum::{
    Json,
    extract::{Path, Query, State},
};
use tracing::instrument;

use super::path::DividendPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::dividend::DividendService,
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        dex::tokens::DexTokenListResponse,
        dividend::{DividendHoldersResponse, DividendTokenQuery, DividendTokensResponse},
    },
    utils::{valid_account_id, valid_existing_token_id},
};

/// ② Trade Dividend — holder ranking by cumulative accrued value for a token.
#[utoipa::path(
    get,
    path = DividendPath::GetHolders.docs_str(),
    params(
        ("token_id" = String, Path, description = "Source token address"),
        ("page" = Option<i64>, Query, description = "Page number (>= 1)"),
        ("limit" = Option<i64>, Query, description = "Page size (1..=100)")
    ),
    responses(
        (status = 200, description = "Dividend holder ranking fetched successfully", body = DividendHoldersResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dividend"
)]
#[instrument(skip(state))]
pub async fn get_dividend_holders(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<DividendHoldersResponse> {
    let token_id = valid_existing_token_id(&state, &token_id).await?;

    let service = DividendService::new(state.postgres.clone());
    let response = service.get_dividend_holders(&token_id, &query).await?;

    Ok(Json(response))
}

/// ① Profile Dividend — a wallet's dividend-bearing tokens with claim info.
#[utoipa::path(
    get,
    path = DividendPath::GetProfile.docs_str(),
    params(
        ("account_id" = String, Path, description = "Wallet address"),
        ("page" = Option<i64>, Query, description = "Page number (>= 1)"),
        ("limit" = Option<i64>, Query, description = "Page size (1..=100)")
    ),
    responses(
        (status = 200, description = "Profile dividends fetched successfully", body = DividendTokensResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dividend"
)]
#[instrument(skip(state))]
pub async fn get_profile_dividends(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<DividendTokensResponse> {
    let account_id = valid_account_id(&account_id)
        .ok_or_else(|| AppError::BadRequest("Invalid account ID".to_string()))?;

    let service = DividendService::new(state.postgres.clone());
    let response = service.get_profile_dividends(&account_id, &query).await?;

    Ok(Json(response))
}

/// Dividend token search — candidate payout tokens (whitelist ∪ V1-graduated ∪ V2).
#[utoipa::path(
    get,
    path = DividendPath::GetTokens.docs_str(),
    params(DividendTokenQuery),
    responses(
        (status = 200, description = "Dividend token candidates fetched successfully", body = DexTokenListResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dividend"
)]
#[instrument(skip(state))]
pub async fn get_dividend_tokens(
    State(state): State<AppState>,
    Query(query): Query<DividendTokenQuery>,
) -> AppJsonResult<DexTokenListResponse> {
    let service = DividendService::new(state.postgres.clone());
    let response = service.get_dividend_tokens(&query).await?;

    Ok(Json(response))
}
