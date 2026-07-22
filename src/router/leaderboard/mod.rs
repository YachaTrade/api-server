pub mod handler;
pub mod path;

use crate::state::AppState;
use axum::{Router, routing::get};
use path::LeaderboardPath;

pub fn router() -> Router<AppState> {
    Router::new().route(
        LeaderboardPath::GetPnlLeaderboard.as_str(),
        get(handler::get_pnl_leaderboard),
    )
}
