use api_server::{
    config::{HTTP_GET_TIMEOUT_MS, HTTP_POST_TIMEOUT_MS},
    cors::get_cors,
    middleware::authenticate_user,
    router::{
        self, account, auth, terminal, health, hype, metadata, metrics, new_event, order, profile,
        raffle, search, token, trade, trend,
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
use tracing::info;
use utoipa::OpenApi;

use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(

    paths(
        // ----------------Auth----------------
        router::auth::handler::auth_nonce,
        router::auth::handler::auth_session,
        router::auth::handler::auth_delete_session,
        // ----------------Account----------------
        router::account::handler::update_account,
        router::account::handler::get_account,
        router::account::handler::connect_x,
        router::account::handler::disconnect_x,
        router::account::handler::update_x,
        router::account::handler::upload_image,
        router::account::handler::register_wallet,
        router::account::handler::get_wallet,

        // ----------------Profile----------------
        router::profile::handler::get_profile,
        router::profile::handler::get_hold_token,
        router::profile::handler::get_token_created,
        router::profile::handler::get_swap_history,
        router::profile::handler::get_point_history,

        // ----------------Search----------------
        router::search::handler::search,

        // ----------------Token----------------
        router::token::handler::get_token,
        router::token::handler::get_token_metadata,
        router::token::handler::salt,

        // ----------------Hype----------------
        router::hype::handler::get_hype_token,
        router::hype::handler::get_hype_token_latest,
        router::hype::handler::get_hype_point,
        router::hype::handler::get_hype_epoch,
        router::hype::handler::get_hype_vote_history,
        router::hype::handler::get_hype_reward_add_history,
        router::hype::handler::vote,
        router::hype::handler::get_total_hype_point,
        router::hype::handler::get_community_treasury,

        // ----------------Trade----------------
        router::trade::handler::get_swap_history,
        router::trade::handler::get_market,
        router::trade::handler::get_prices,
        router::trade::handler::get_holder,
        router::trade::handler::get_metrics,

        // ----------------Search----------------
        router::search::handler::search,

        // ----------------Order----------------
        router::order::handler::get_creation_time_order,
        router::order::handler::get_market_cap_order,
        router::order::handler::get_latest_trade_order,

        // ----------------New Event----------------
        router::new_event::handler::get_new_event,

        // ----------------Raffle----------------
        router::raffle::handler::get_eligible,
        router::raffle::handler::get_prizes,

        // ----------------Metadata----------------
        router::metadata::handler::upload_image,
        router::metadata::handler::upload_metadata,

        // ----------------Terminal----------------
        router::terminal::handler::get_latest_block,
        router::terminal::handler::get_asset,
        router::terminal::handler::get_pair,
        router::terminal::handler::get_events,
        router::terminal::handler::get_terminal_metadata,

        // ----------------Trend----------------
        router::trend::handler::get_trend,
        router::trend::handler::insert_trend,

    ),
    components(
        schemas(
            // Common
            types::common::info::TokenInfo,
            types::common::info::AccountInfo,
            types::common::info::MarketInfo,
            types::common::info::MarketType,
            types::common::info::BalanceInfo,
            types::common::info::SwapInfo,
            types::common::info::SwapType,
            types::common::info::RewardInfo,
            types::common::info::TokenWithBalanceInfo,
            types::common::info::TokenSwapInfo,
            types::common::info::TokenCreatedInfo,
            types::common::pagination::PaginationParams,
            types::common::identifier::Identifier,
            // Auth
            types::auth::AuthNonceRequest,
            types::auth::AuthNonceResponse,
            types::auth::AuthSessionRequest,
            types::auth::AuthSessionResponse,

            // Account
            types::account::AccountResponse,
            types::account::UpdateAccountRequest,
            types::account::ConnectXRequest,
            types::account::UpdateXRequest,
            types::account::UploadImageResponse,
            types::account::RegisterWalletRequest,
            types::account::GetWalletResponse,

            // Token
            types::token::TokenResponse,
            types::token::order::TokenOrderType,
            types::token::order::OrderToken,
            types::token::order::OrderQuery,
            types::token::order::OrderTokenResponse,
            types::token::metadata::TokenMetadataResponse,
            types::token::salt::MineSaltRequest,
            types::token::salt::MineSaltResponse,
            types::token::salt::MineSaltError,

            // Metadata
            types::metadata::UploadImageMultipart,
            types::metadata::UploadImageResponse,
            types::metadata::UploadMetadataRequest,
            types::metadata::UploadMetadataResponse,
            types::metadata::TokenMetadata,
            types::metadata::TerminalMetadataResponse,

            // Terminal
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

            //Hype
            types::hype::HypeToken,
            types::hype::HypeTokenResponse,
            types::hype::HypeInfo,
            types::hype::HypePointResponse,
            types::hype::HypeEpochResponse,
            types::hype::HypeVoteHistory,
            types::hype::HypeVoteHistoryResponse,
            types::hype::HypeVoteRequest,
            types::hype::HypeVoteResponse,
            types::hype::RewardAdd,
            types::hype::HypeRewardAddHistoryResponse,
            types::hype::AmountResponse,

            //Trading
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

            //Profile
            types::profile::ProfileResponse,
            types::profile::HoldTokenResponse,
            types::profile::SwapHistoryResponse,
            types::profile::CreatedTokensResponse,
            types::profile::PointRecord,
            types::profile::PointRecordTotal,
            types::profile::PointHistoryResponse,

            //Search
            types::search::TokenSearchResult,
            types::search::TokenSearchResponse,
            types::search::AccountSearchResult,
            types::search::AccountSearchResponse,
            types::search::SearchResponse,

            // New Event
            types::new_event::NewEventResponse,
            types::new_event::NewEvent,
            types::new_event::EventType,

            // Raffle
            types::raffle::RaffleStatusResponse,
            types::raffle::Prize,
            types::raffle::PrizeListResponse,

            // Trend
            types::trend::TrendToken,
            types::trend::TrendResponse,
            types::trend::TrendRequest,
            types::trend::TrendActionResponse,

        )
    ),
    tags(
        (name="Auth",description = "Authentication endpoints"),
        (name="Account",description="Account management endpoints"),
        (name="Follow",description="Follow management endpoints"),
        (name="Token",description="Token management endpoints"),
        (name="Profile",description="Profile management endpoints"),
        (name="Search",description="Search endpoints"),
        (name="Order",description="Order endpoints"),
        (name="Hype",description="Hype Token endpoints"),
        (name="New Event",description="New Event endpoints"),
        (name="Raffle",description="Raffle endpoints"),
        (name="Metadata",description="Metadata upload endpoints"),
        (name="Terminal",description="Gecko Terminal API endpoints"),
        (name="Trend",description="Trend token endpoints"),
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub struct ApiDoc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port number for the server (default: 3000, can be overridden by HTTP_PORT env var)
    #[arg(short, long, default_value = "8000")]
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
    // 우선순위: 1. 커맨드 라인 인자 2. 환경변수 3. 기본값(8000)
    let port = args
        .port
        .map(|p| p.to_string())
        .or_else(|| std::env::var("HTTP_PORT").ok())
        .unwrap_or_else(|| "8000".to_string());

    info!("Server will start on {}:{} - v2 deployment test", ip, port);

    let app_state = AppState::new().await;
    info!("AppState initialized");

    let cookie_manager_layer = CookieManagerLayer::new();
    let root = Router::new().route("/", get(|| async { "Hello, World!" }));
    let app = Router::new()
        .merge(root)
        .merge(health::router())
        .merge(auth::router(app_state.clone()))
        .merge(account::router(app_state.clone()))
        .merge(raffle::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(token::router())
        .merge(search::router())
        .merge(trade::router())
        .merge(profile::router(app_state.clone()))
        .merge(order::router())
        .merge(hype::router(app_state.clone()))
        // .merge(bot::router()) // bot 모듈이 존재하지 않음
        .merge(new_event::router())
        .merge(metadata::router())
        .merge(metrics::router())
        .merge(terminal::router())
        .merge(trend::router(app_state.clone()))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(DefaultBodyLimit::max(100_000)) // 100KB global limit (image upload has separate 5MB limit)
        .layer(axum_middleware::from_fn(method_based_timeout))
        .layer(ServiceBuilder::new().layer(get_cors()).into_inner())
        .layer(cookie_manager_layer)
        // .layer(GovernorLayer {
        //     config: governor_conf,
        // })
        .with_state(app_state)
        .fallback(handler_404);

    let addr = SocketAddr::from((
        IpAddr::from_str(ip.as_str()).unwrap(),
        port.parse().unwrap(),
    ));

    info!("Listening on {} Server port{}", addr, port);

    axum_server::bind(addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
    Ok(())
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

// 메서드별 타임아웃 미들웨어
async fn method_based_timeout(
    method: Method,
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    // Exempt upload endpoints from timeout restrictions
    let path = req.uri().path();
    let is_upload_endpoint =
        path.starts_with("/metadata/image") || path.starts_with("/metadata/metadata");

    if is_upload_endpoint {
        // No timeout for upload endpoints
        return Ok(next.run(req).await);
    }

    let timeout_duration = match method {
        Method::GET => Duration::from_millis(*HTTP_GET_TIMEOUT_MS),
        _ => Duration::from_millis(*HTTP_POST_TIMEOUT_MS), // POST, PUT, DELETE 등
    };

    match tokio::time::timeout(timeout_duration, next.run(req)).await {
        Ok(response) => Ok(response),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}
