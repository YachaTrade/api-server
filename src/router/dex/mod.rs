pub mod path;
pub mod pool;
pub mod positions;

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
}
