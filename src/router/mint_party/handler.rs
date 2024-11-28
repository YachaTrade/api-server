use axum::{extract::State, Extension, Json};
use rayon::iter::Update;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::{schema, ToSchema};

use crate::{
    db::postgres::{controller::mint_party::MintPartyController, model::MintParty},
    result::{AppError, AppJsonResult},
    state::AppState,
};
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMintPartyRequest {
    #[schema(example = "MintParty Create Transaction Hash")]
    tx: String,
    #[schema(example = "This Token is good meme token.")]
    description: String,
    #[schema(example = "null,https://twitter.com/meme_token")]
    twitter: Option<String>,
    #[schema(example = "null,https://t.me/meme_token")]
    telegram: Option<String>,
    #[schema(example = "null,https://meme_token.org")]
    website: Option<String>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateMintPartyResponse {
    mint_party: MintParty,
}

pub async fn update_mint_party(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<UpdateMintPartyRequest>,
) -> AppJsonResult<UpdateMintPartyResponse> {
    let UpdateMintPartyRequest {
        tx,
        description,
        twitter,
        telegram,
        website,
    } = payload;

    let mint_party_controller = MintPartyController::new(state.postgres.clone());
    let mint_party = mint_party_controller
        .get_mint_party_tx(tx.clone())
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    let creator = mint_party.account_id;
    if creator != session_address {
        info!(
            "Bad Request: Unauthroized MintParty Update. creator = {:?}, session_address = {:?}",
            creator, session_address
        );
        return Err(AppError::BadRequest(
            "Unauthroized MintParty Update".to_string(),
        ));
    }

    let mint_party = mint_party_controller
        .update_mint_party_metadata(tx.clone(), description, twitter, telegram, website, creator)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(UpdateMintPartyResponse { mint_party }))
}
