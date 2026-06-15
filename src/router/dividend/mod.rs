pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::DividendPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            DividendPath::GetTokens.as_str(),
            get(handler::get_dividend_tokens),
        )
        .route(
            DividendPath::GetHolders.as_str(),
            get(handler::get_dividend_holders),
        )
        .route(
            DividendPath::GetVault.as_str(),
            get(handler::get_dividend_vault),
        )
        .route(
            DividendPath::GetProfile.as_str(),
            get(handler::get_profile_dividends),
        )
}

#[cfg(test)]
mod tests {
    /// Builds the router so matchit validates route registration — catches any
    /// static/param conflict (e.g. /dividend/tokens vs /dividend/:token_id).
    #[test]
    fn router_builds_without_route_conflict() {
        let _ = super::router();
    }
}
