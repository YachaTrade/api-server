use axum::{middleware, routing::get, Router};
use path::{ActiveUserPath, ScorePath};

use crate::{middleware::authenticate_user, state::AppState};

pub mod handler;
pub mod path;

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            ActiveUserPath::CheckActiveUser.as_str(),
            get(handler::check_active_user),
        )
        .route(ScorePath::GetTop.as_str(), get(handler::get_top_point))
        .route(
            ScorePath::GetAccountPoint.as_str(),
            get(handler::get_score_by_account_id)
                .layer(middleware::from_fn_with_state(app_state, authenticate_user)),
        )
}
