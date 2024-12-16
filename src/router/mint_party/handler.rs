use axum::{
    extract::{Query, State},
    Extension, Json,
};

use serde::{Deserialize, Serialize};

use tracing::info;
use utoipa::{schema, ToSchema};

use crate::{
    db::postgres::{controller::mint_party::MintPartyController, model::MintParty},
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        order_type::{MintPartyOrderType, OrderDirection},
        response::{MintPartyBalance, MintPartyDepositList, MintPartyResponse},
    },
};

use super::path::MintPartyPath;

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
/// Update description and social links of a mint party
#[utoipa::path(
    put,
    path = MintPartyPath::UpdateMintParty.docs_str(),
    request_body = UpdateMintPartyRequest,
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Successfully updated mint party", body = UpdateMintPartyResponse),
        (status = 400, description = "Bad request - Invalid input or unauthorized update"),
        (status = 401, description = "Unauthorized - Invalid or missing session cookie"),
        (status = 403, description = "Forbidden - User does not have permission to update this mint party"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_cookie" = [])
    ),
    tag = "MintParty"
)]
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
        .update_mint_party_metadata(
            tx.clone(),
            description,
            twitter,
            telegram,
            website,
            session_address,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(UpdateMintPartyResponse { mint_party }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MintPartyListResponse {
    pub mint_party: Vec<MintPartyResponse>,
}

/// Get recently joined mint parties
///
/// Returns a list of mint parties that were recently joined by users
#[utoipa::path(
    get,
    path = MintPartyPath::GetLastJoinMintParty.docs_str(),
    responses(
        (status = 200, description = "Successfully retrieved last joined mint parties", body = MintPartyListResponse),
        (status = 500, description = "Internal server error")
    ),
    tag="MintParty"
)]
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

/// Get mint party list
///
/// Returns a paginated list of mint parties with optional ordering
#[utoipa::path(
    get,
    path = MintPartyPath::GetMintPartyList.docs_str(),
    params(
        ("order_type" = MintPartyOrderType, Query, description = "Order by creation time or funding amount"),
        ("direction" = OrderDirection, Query, description = "Sort direction (asc/desc)"),
        ("pagination" = Option<i16>, Query, description = "Pagination offset")
    ),
    responses(
        (status = 200, description = "Successfully retrieved mint party list", body = MintPartyListResponse),
        (status = 500, description = "Internal server error")
    ),
    tag="MintParty"
)]
pub async fn get_mint_party_list(
    State(state): State<AppState>,
    Query(query): Query<MintPartyQuery>,
) -> AppJsonResult<MintPartyListResponse> {
    let paginator = query.pagination.unwrap_or(0);
    let order_type = query.order_type;
    let direction = query.direction;
    let mint_party_controller = MintPartyController::new(state.postgres.clone());
    let mint_party_list = mint_party_controller
        .get_mint_partys(order_type, direction, Some(paginator))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(MintPartyListResponse {
        mint_party: mint_party_list,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MintPartyDepositListResopnse {
    pub deposit_list: Vec<MintPartyDepositList>,
}

/// Get mint party deposit list
///
/// Returns a list of deposits made by the authenticated user
#[utoipa::path(
    get,
    path = MintPartyPath::GetMintPartyDepositList.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
     ),
    responses(
        (status = 200, description = "Successfully retrieved deposit list", body = MintPartyDepositListResopnse),
        (status = 401, description = "Unauthorized - Invalid or missing session cookie"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_cookie" = [])
    ),
    tag="MintParty"
)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct MintPartyBalanceResponse {
    mint_party_balances: Vec<MintPartyBalance>,
}

/// Get mint party balance
///
/// Returns the user's balance across all mint parties
#[utoipa::path(
    get,
    path = MintPartyPath::GetMintPartyBalanceList.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session token for authentication")
     ),
    responses(
        (status = 200, description = "Successfully retrieved balance information", body = MintPartyBalanceResponse),
        (status = 401, description = "Unauthorized - Invalid or missing session cookie"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_cookie" = [])
    ),
    tag="MintParty"
)]
pub async fn get_mint_party_balance(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<MintPartyBalanceResponse> {
    let mint_party_controller = MintPartyController::new(state.postgres.clone());
    let mint_party_balances = mint_party_controller
        .get_mint_party_balance(session_address)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(Json(MintPartyBalanceResponse {
        mint_party_balances,
    }))
}
