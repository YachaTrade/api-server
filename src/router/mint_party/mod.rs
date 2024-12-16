use axum::{
    middleware,
    routing::{get, put},
    Router,
};
use path::MintPartyPath;

use crate::{middleware::authenticate_user, state::AppState};

pub mod handler;
pub mod path;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            MintPartyPath::UpdateMintParty.as_str(),
            put(handler::update_mint_party).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            MintPartyPath::GetLastJoinMintParty.as_str(),
            get(handler::get_last_join_mint_party),
        )
        .route(
            MintPartyPath::GetMintPartyList.as_str(),
            get(handler::get_mint_party_list),
        )
        .route(
            MintPartyPath::GetMintPartyDepositList.as_str(),
            get(handler::get_mint_party_deposit_list),
        )
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            authenticate_user,
        ))
        .route(
            MintPartyPath::GetMintPartyBalanceList.as_str(),
            get(handler::get_mint_party_balance),
        )
        .layer(middleware::from_fn_with_state(app_state, authenticate_user))
}
