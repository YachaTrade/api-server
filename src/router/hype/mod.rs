pub mod path;

pub mod handler;
use axum::{
    Router, middleware,
    routing::{get, post},
};

use path::HypePath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(HypePath::GetHype.as_str(), get(handler::get_hype_token))
        .route(HypePath::GetEpoch.as_str(), get(handler::get_hype_epoch))
        .route(
            HypePath::GetTotalSpendPoint.as_str(),
            get(handler::get_total_spend_point),
        )
        .route(
            HypePath::GetCommunityTreasury.as_str(),
            get(handler::get_community_treasury),
        )
        .route(
            HypePath::GetPoint.as_str(),
            get(handler::get_hype_point).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            HypePath::GetVoteHistory.as_str(),
            get(handler::get_hype_vote_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            HypePath::GetPointHistory.as_str(),
            get(handler::get_hype_point_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            HypePath::GetRewardHistory.as_str(),
            get(handler::get_hype_reward_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            HypePath::GetRewardAddHistory.as_str(),
            get(handler::get_hype_reward_add_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            HypePath::Vote.as_str(),
            post(handler::vote).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
}
