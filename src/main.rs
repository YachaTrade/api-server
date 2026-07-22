use api_server::{
    config::{HTTP_GET_TIMEOUT_MS, HTTP_POST_TIMEOUT_MS},
    cors::get_cors,
    router::{
        self, account, agent, api_key, auth, cms, health, leaderboard, metadata, metrics,
        new_event, order, profile, search, terminal, token, trade, trend,
    },
    state::AppState,
    types,
};

use std::{
    env,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use anyhow::Result;
use axum::http::Request;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{Method, StatusCode},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::get,
};
use clap::Parser;
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tracing::{info, warn};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        router::auth::handler::auth_nonce,
        router::auth::handler::auth_session,
        router::auth::handler::auth_delete_session,

        router::account::handler::update_account,
        router::account::handler::get_account,
        router::account::handler::upload_image,
        router::account::handler::register_wallet,
        router::account::handler::get_wallet,

        router::profile::handler::get_profile,
        router::profile::handler::get_hold_token,
        router::profile::handler::get_token_created,
        router::profile::handler::get_swap_history,

        router::search::handler::search,

        router::token::handler::get_token,
        router::token::handler::get_token_metadata,
        router::token::handler::salt,

        router::trade::handler::get_swap_history,
        router::trade::handler::get_market,
        router::trade::handler::get_prices,
        router::trade::handler::get_holder,
        router::trade::handler::get_metrics,

        router::order::handler::get_creation_time_order,
        router::order::handler::get_market_cap_order,
        router::order::handler::get_latest_trade_order,

        router::new_event::handler::get_new_event,

        router::metadata::handler::upload_image,
        router::metadata::handler::upload_metadata,

        router::terminal::handler::get_latest_block,
        router::terminal::handler::get_asset,
        router::terminal::handler::get_pair,
        router::terminal::handler::get_events,
        router::terminal::handler::get_terminal_metadata,

        router::trend::handler::get_trend,
        router::leaderboard::handler::get_pnl_leaderboard,

        router::cms::handler::set_nsfw,
        router::cms::handler::insert_trend,
        router::cms::handler::update_metadata,

        router::cms::analytics::handler::get_churned_users,
        router::cms::analytics::handler::get_active_users,
        router::cms::analytics::handler::get_new_users,
        router::cms::analytics::handler::get_user_roi,

        router::agent::handler::get_chart,
        router::agent::handler::get_swap_history,
        router::agent::handler::get_market,
        router::agent::handler::get_metrics,
        router::agent::handler::get_token,
        router::agent::handler::get_holdings,
        router::agent::handler::upload_image,
        router::agent::handler::upload_metadata,
        router::agent::handler::get_tokens_created,
        router::agent::handler::salt,
    ),
    components(
        schemas(
            types::common::info::TokenInfo,
            types::common::info::AccountInfo,
            types::common::info::MarketInfo,
            types::common::info::MarketType,
            types::common::info::QuoteInfo,
            types::common::info::BalanceInfo,
            types::common::info::SwapInfo,
            types::common::info::SwapType,
            types::common::info::TokenWithBalanceInfo,
            types::common::info::TokenSwapInfo,
            types::common::info::TokenCreatedInfo,
            types::common::pagination::PaginationParams,
            types::common::identifier::Identifier,

            types::auth::AuthNonceRequest,
            types::auth::AuthNonceResponse,
            types::auth::AuthSessionRequest,
            types::auth::AuthSessionResponse,

            types::account::AccountResponse,
            types::account::UpdateAccountRequest,
            types::account::UploadImageResponse,
            types::account::RegisterWalletRequest,
            types::account::GetWalletResponse,

            types::token::TokenResponse,
            types::token::order::TokenOrderType,
            types::token::order::OrderToken,
            types::token::order::OrderQuery,
            types::token::order::OrderTokenResponse,
            types::token::metadata::TokenMetadataResponse,
            types::token::salt::MineSaltRequest,
            types::token::salt::MineSaltResponse,
            types::token::salt::MineSaltError,

            types::metadata::UploadImageMultipart,
            types::metadata::UploadImageResponse,
            types::metadata::UploadMetadataRequest,
            types::metadata::UploadMetadataResponse,
            types::metadata::TokenMetadata,
            types::metadata::TerminalMetadataResponse,

            types::terminal::Block,
            types::terminal::LatestBlockResponse,
            types::terminal::Asset,
            types::terminal::AssetResponse,
            types::terminal::AssetQuery,
            types::terminal::Pair,
            types::terminal::PairResponse,
            types::terminal::PairQuery,
            types::terminal::Reserves,
            types::terminal::Event,
            types::terminal::EventsResponse,
            types::terminal::EventsQuery,

            types::trading::chart::Chart,
            types::trading::chart::ChartResponse,
            types::trading::position::TokenHolder,
            types::trading::position::TokenHolderResponse,
            types::trading::swap_history::TokenSwap,
            types::trading::swap_history::TokenSwapResponse,
            types::trading::swap_history::VolumeRange,
            types::trading::market::MarketResponse,
            types::trading::chart::BarResponse,
            types::trading::chart::GetBarsRequest,
            types::trading::metrics::TransactionCount,
            types::trading::metrics::VolumeAmount,
            types::trading::metrics::MakerCount,
            types::trading::metrics::MetricItem,
            types::trading::metrics::MetricsBatchResponse,

            types::profile::ProfileResponse,
            types::profile::HoldTokenResponse,
            types::profile::SwapHistoryResponse,
            types::profile::CreatedTokensResponse,

            types::search::TokenSearchResult,
            types::search::TokenSearchResponse,
            types::search::AccountSearchResult,
            types::search::AccountSearchResponse,
            types::search::SearchResponse,

            types::new_event::NewEventResponse,
            types::new_event::NewEvent,
            types::new_event::EventType,

            types::trend::TrendToken,
            types::trend::TrendResponse,
            types::trend::TrendRequest,
            types::trend::TrendActionResponse,

            types::leaderboard::LeaderboardQuery,
            types::leaderboard::Pnl,
            types::leaderboard::PnlLeaderboardEntry,
            types::leaderboard::PnlLeaderboardResponse,

            types::cms::SetNsfwRequest,
            types::cms::InsertTrendRequest,
            types::cms::CmsActionResponse,
            types::cms::UpdateMetadataRequest,
            types::cms::UpdateMetadataResponse,
            router::cms::handler::UpdateMetadataMultipart,

            types::cms::analytics::TopHeldToken,
            types::cms::analytics::UserActivityResponse,
            types::cms::analytics::NewUsersResponse,
            types::cms::analytics::UserRoiResponse,
        )
    ),
    tags(
        (name="Auth", description="Authentication endpoints"),
        (name="Account", description="Account management endpoints"),
        (name="Token", description="Token management endpoints"),
        (name="Profile", description="Profile management endpoints"),
        (name="Search", description="Search endpoints"),
        (name="Order", description="Order endpoints"),
        (name="New Event", description="New Event endpoints"),
        (name="Metadata", description="Metadata upload endpoints"),
        (name="Terminal", description="Gecko Terminal API endpoints"),
        (name="Trend", description="Trend token endpoints"),
        (name="Leaderboard", description="Leaderboard endpoints"),
        (name="CMS", description="CMS admin endpoints"),
        (name="CMS Analytics", description="CMS analytics dashboard endpoints"),
        (name="Agent", description="Agent API endpoints for AI integrations"),
    ),
    security(("session_cookie" = []))
)]
pub struct ApiDoc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port number. Falls back to PORT and then HTTP_PORT when omitted.
    #[arg(short, long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    info!("API server started");
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    let ip = env::var("IP").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = args
        .port
        .map(|port| port.to_string())
        .or_else(|| env::var("PORT").ok())
        .or_else(|| env::var("HTTP_PORT").ok())
        .expect("PORT must be set with --port, PORT, or HTTP_PORT");

    info!("Server will start on {}:{}", ip, port);

    let app_state = AppState::new().await;
    info!("AppState initialized");

    if let Err(error) = app_state.redis.flush_all().await {
        warn!("Failed to flush Redis: {}", error);
    }

    let sync_state = app_state.clone();
    tokio::spawn(async move {
        use api_server::services::api_key::sync_api_key_usage_to_db;
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            match sync_api_key_usage_to_db(&sync_state.postgres, &sync_state.redis).await {
                Ok(count) if count > 0 => info!("API key usage synced to DB: {} keys", count),
                Ok(_) => {}
                Err(error) => warn!("Failed to sync API key usage: {:?}", error),
            }
        }
    });

    let app = Router::new()
        .merge(Router::new().route("/", get(|| async { "Hello, World!" })))
        .merge(health::router())
        .merge(auth::router(app_state.clone()))
        .merge(account::router(app_state.clone()))
        .merge(token::router())
        .merge(search::router())
        .merge(trade::router())
        .merge(profile::router())
        .merge(order::router())
        .merge(new_event::router())
        .merge(metadata::router())
        .merge(metrics::router())
        .merge(terminal::router())
        .merge(trend::router(app_state.clone()))
        .merge(leaderboard::router())
        .merge(api_key::router(app_state.clone()))
        .merge(cms::router(app_state.clone()))
        .merge(agent::router())
        .merge(SwaggerUi::new("/dev-sw").url("/dev-sw/openapi.json", ApiDoc::openapi()))
        .merge(Scalar::with_url("/dev-scalar", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(100_000))
        .layer(axum_middleware::from_fn(method_based_timeout))
        .layer(axum_middleware::from_fn_with_state(
            app_state.clone(),
            api_server::middleware::api_key_gate,
        ))
        .layer(ServiceBuilder::new().layer(get_cors()).into_inner())
        .layer(CookieManagerLayer::new())
        .with_state(app_state)
        .fallback(handler_404);

    let ip_addr = IpAddr::from_str(ip.as_str())
        .map_err(|error| anyhow::anyhow!("Invalid IP address '{}': {}", ip, error))?;
    let port_num: u16 = port
        .parse()
        .map_err(|error| anyhow::anyhow!("Invalid port '{}': {}", port, error))?;
    let addr = SocketAddr::from((ip_addr, port_num));

    info!("Listening on {}", addr);
    axum_server::bind(addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|error| anyhow::anyhow!("Server failed to start: {}", error))?;
    Ok(())
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

async fn method_based_timeout(
    method: Method,
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let path = req.uri().path();
    let is_upload_endpoint = path.starts_with("/metadata/image")
        || path.starts_with("/metadata/metadata")
        || path.starts_with("/agent/token/image");

    if is_upload_endpoint {
        return Ok(next.run(req).await);
    }

    let timeout_duration = match method {
        Method::GET => Duration::from_millis(*HTTP_GET_TIMEOUT_MS),
        _ => Duration::from_millis(*HTTP_POST_TIMEOUT_MS),
    };

    match tokio::time::timeout(timeout_duration, next.run(req)).await {
        Ok(response) => Ok(response),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_exposes_only_giwa_product_routes() {
        let openapi = ApiDoc::openapi();

        for path in [
            "/hype/token",
            "/chester/round",
            "/raffle/round",
            "/dev-post",
            "/dex/tokens",
            "/vault/{token_id}",
            "/dividend/tokens",
            "/quote_token",
            "/profile/point-history",
            "/profile/gift-fee/{account_id}",
            "/leaderboard/hype_point",
            "/cms/analytics/chester-retention",
            "/account/update_x",
            "/x/oauth/login",
            "/trade/xinfo/{token_id}",
        ] {
            assert!(
                !openapi.paths.paths.contains_key(path),
                "retired path remains: {path}"
            );
        }

        for path in [
            "/profile/{account_id}",
            "/leaderboard/pnl",
            "/cms/analytics/new-users",
        ] {
            assert!(
                openapi.paths.paths.contains_key(path),
                "retained path missing: {path}"
            );
        }

        let tags = openapi.tags.unwrap_or_default();
        for name in [
            "Hype",
            "Chester",
            "Raffle",
            "DevPost",
            "Dex",
            "Vault",
            "Dividend",
            "XVerification",
        ] {
            assert!(
                !tags.iter().any(|tag| tag.name == name),
                "retired tag remains: {name}"
            );
        }
    }
}
