pub mod path;
pub mod pool;
pub mod positions;
pub mod search;
pub mod tokens;

use axum::{Router, middleware, routing::get};

use path::DexPath;

use crate::{middleware::authenticate_user, state::AppState};

pub fn router(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            DexPath::GetPositions.as_str(),
            get(positions::get_positions),
        )
        .route(DexPath::GetPool.as_str(), get(pool::get_pool))
        .route(DexPath::GetTokens.as_str(), get(tokens::get_tokens))
        .route(
            DexPath::SearchTokens.as_str(),
            get(search::search_tokens).layer(middleware::from_fn_with_state(
                app_state.clone(),
                authenticate_user,
            )),
        )
}
