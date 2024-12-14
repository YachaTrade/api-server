use axum::{
    extract::{Query, State},
    Extension, Json,
};

use serde::{Deserialize, Serialize};

use tracing::info;
use utoipa::{schema, ToSchema};

use crate::{
    db::postgres::{
        controller::mint_party::{self, MintPartyController},
        model::MintParty,
    },
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        order_type::{MintPartyOrderType, OrderDirection},
        response::{MintPartyDepositList, MintPartyResponse},
    },
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

#[derive(Debug, Serialize, ToSchema)]
pub struct MintPartyListResponse {
    pub mint_party: Vec<MintPartyResponse>,
}
pub async fn get_last_join_mint_party(
    State(state): State<AppState>,
) -> AppJsonResult<MintPartyListResponse> {
    let mint_party_controller = MintPartyController::new(state.postgres.clone());
    let last_join_mint_party = mint_party_controller
        .get_last_join_mint_party()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(MintPartyListResponse {
        mint_party: last_join_mint_party,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MintPartyQuery {
    #[serde(rename = "order_type", default)]
    pub order_type: MintPartyOrderType,
    #[serde(default)]
    pub direction: OrderDirection,
    pub pagination: Option<i16>,
}

pub async fn get_mint_party_list(
    State(state): State<AppState>,
    Query(query): Query<MintPartyQuery>,
) -> AppJsonResult<MintPartyListResponse> {
    let mint_party_controller = MintPartyController::new(state.postgres.clone());
    let mint_party_list = mint_party_controller
        .get_mint_partys(
            query.order_type,
            query.direction,
            Some(query.pagination.unwrap_or(0)),
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(MintPartyListResponse {
        mint_party: mint_party_list,
    }))
}

// pub async fn get_mint_party_deposit_list(State(state): State<AppState>) -> AppJsonResult<todo!()> {

// }

#[derive(Debug, Serialize, ToSchema)]
pub struct MintPartyDepositListResopnse {
    pub deposit_list: Vec<MintPartyDepositList>,
}
pub async fn get_mint_party_deposit_list(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<MintPartyDepositListResopnse> {
    let mint_party_controller = MintPartyController::new(state.postgres.clone());

    let mint_party_deposit_list = mint_party_controller
        .get_mint_party_deposit_list(session_address)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(Json(MintPartyDepositListResopnse {
        deposit_list: mint_party_deposit_list,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MintPartyBalanceResponse {}

pub async fn get_mint_party_balance(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<MintPartyBalanceResponse> {
    let mint_party_controller = MintPartyController::new(state.postgres.clone());

    Ok(Json(MintPartyBalanceResponse {}))
}
