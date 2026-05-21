pub mod path;
pub mod pool;
pub mod positions;
pub mod tokens;

use axum::{Router, routing::get};

use path::DexPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            DexPath::GetPositions.as_str(),
            get(positions::get_positions),
        )
        .route(DexPath::GetPool.as_str(), get(pool::get_pool))
        .route(DexPath::GetTokens.as_str(), get(tokens::get_tokens))
}
