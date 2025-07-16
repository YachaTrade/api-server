pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::TradePath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            TradePath::GetSwapHistory.as_str(),
            get(handler::get_swap_history),
        )
        .route(TradePath::GetHolder.as_str(), get(handler::get_holder))
        .route(TradePath::GetMarket.as_str(), get(handler::get_market))
        .route(TradePath::GetChart.as_str(), get(handler::get_prices))
        .route(TradePath::GetPrice.as_str(), get(handler::get_price))
        .route(
            TradePath::GetManagementHistory.as_str(),
            get(handler::get_management_history),
        )
}
