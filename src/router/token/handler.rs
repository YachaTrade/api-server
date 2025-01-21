use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use tracing::info;
use utoipa::ToSchema;

use super::path::TokenPath;
use crate::{
    db::postgres::{controller::token::TokenController, model::Token},
    result::{AppError, AppJsonResult},
    state::AppState,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponse {
    token: Token,
}
/// Get token metadata
#[utoipa::path(
    get,
    path = TokenPath::GetToken.docs_str(),
    params(
        ("token" = String, Path, description = "Token address")
    ),
    responses(
        (status = 200, description = "Token fetched successfully", body = TokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Token"
)]
pub async fn get_token(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppJsonResult<TokenResponse> {
    info!("Get token Request for token: {}", token);
    let token_controller = TokenController::new(state.postgres.clone());
    let token = token_controller
        .get_token(token)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;
    Ok(Json(TokenResponse { token }))
}
