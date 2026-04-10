pub mod handler;
pub mod path;

use axum::{Router, middleware, routing::get};

use crate::{middleware::authenticate_user, state::AppState};

use path::ChesterPath;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(ChesterPath::Volume.as_str(), get(handler::get_volume))
        .route(ChesterPath::Round.as_str(), get(handler::get_round))
        .route(ChesterPath::Rewards.as_str(), get(handler::get_rewards))
        .route(
            ChesterPath::BoxRewards.as_str(),
            get(handler::get_box_rewards).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
        .route(
            ChesterPath::SwapHistory.as_str(),
            get(handler::get_swap_history),
        )
        .route(
            ChesterPath::RewardHistory.as_str(),
            get(handler::get_reward_history).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
}
