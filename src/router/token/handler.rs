use std::time::Duration;

use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{info, instrument};
use utoipa::ToSchema;

use crate::{
    db::postgres::{controller::token::TokenController, model::Token},
    result::{AppError, AppJsonResult},
    state::AppState,
};

use super::path::Path;

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTokenRequest {
    #[schema(example = "Meme Token Create Transaction Hash")]
    tx: String,
    #[schema(example = "Memetoken is a decentralized digital currency.")]
    description: String,
    #[schema(example = "null,https://twitter.com/meme_token")]
    twitter: Option<String>,
    #[schema(example = "null,https://t.me/meme_token")]
    telegram: Option<String>,
    #[schema(example = "null,https://meme_token.org")]
    website: Option<String>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateTokenResponse {
    token: Token,
}

/// Update token metadata
#[utoipa::path(
    put,
    path = Path::UpdateToken.as_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = UpdateTokenRequest,
    responses(
        (status = 200, description = "Token updated successfully", body = UpdateTokenResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Token"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn update_token(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateTokenRequest>,
) -> AppJsonResult<UpdateTokenResponse> {
    let UpdateTokenRequest {
        tx,
        description,
        twitter,
        telegram,
        website,
    } = payload;
    info!("Update token Request by {}", session_address);
    let creator_address = session_address;
    let token_controller = TokenController::new(state.postgres.clone());

    let token = get_token_with_retry(&token_controller, tx.clone()).await?;
    info!("update_token : token = {:?}", token);
    if token.creator != creator_address {
        info!(
            "token.creator = {:?}, creator_address = {:?}",
            token.creator, creator_address
        );
        return Err(AppError::BadRequest("Unauthroized Token".to_string()));
    }

    if token.is_updated {
        return Err(AppError::BadRequest("Token is already updated".to_string()));
    }

    let token = token_controller
        .update_token_metadata(tx, description, twitter, telegram, website, creator_address)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(UpdateTokenResponse { token }))
}

async fn get_token_with_retry(
    token_controller: &TokenController,
    tx: String,
) -> Result<Token, AppError> {
    let max_attempts = 5;
    let mut attempts = 0;
    let mut last_error = None;

    while attempts < max_attempts {
        match token_controller.get_token_tx(tx.clone()).await {
            Ok(token) => return Ok(token),
            Err(err) => {
                last_error = Some(err);
                attempts += 1;
                if attempts < max_attempts {
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    Err(AppError::BadRequest(format!(
        "Get Token failed after {} attempts. Last error: {}",
        max_attempts,
        last_error.unwrap()
    )))
}
