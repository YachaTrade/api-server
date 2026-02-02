pub mod handler;
pub mod path;

use axum::{extract::DefaultBodyLimit, routing::{get, post}, Router};
use path::AgentPath;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(AgentPath::GetChart.as_str(), get(handler::get_chart))
        .route(AgentPath::GetSwapHistory.as_str(), get(handler::get_swap_history))
        .route(AgentPath::GetMarket.as_str(), get(handler::get_market))
        .route(AgentPath::GetMetrics.as_str(), get(handler::get_metrics))
        .route(AgentPath::GetToken.as_str(), get(handler::get_token))
        .route(AgentPath::GetHoldings.as_str(), get(handler::get_holdings))
        .route(
            AgentPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route(
            AgentPath::UploadMetadata.as_str(),
            post(handler::upload_metadata).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route(AgentPath::GetTokensCreated.as_str(), get(handler::get_tokens_created))
}
