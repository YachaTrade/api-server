use api_server::{
    cors::get_cors,
    middleware::authenticate_user,
    router::{self, account, auth,  chart, order, profile, search, thread, token,follow},
    state::AppState,
    types,
};

use std::{
    env, net::{IpAddr, SocketAddr}, str::FromStr, sync::Arc, time::Duration
};

use anyhow::Result;
use axum::{
    error_handling::HandleErrorLayer, http::{Method, StatusCode, Uri}, middleware as axum_middleware, response::IntoResponse, routing::get, BoxError, Router
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, 
};
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use clap::Parser;

#[derive(OpenApi)]
#[openapi(
    paths(
        router::auth::handler::auth_nonce,
        router::auth::handler::auth_session,
        router::auth::handler::auth_delete_session,
        router::account::handler::update_account,
        router::account::handler::get_account,
        router::profile::handler::get_profile,
        router::profile::handler::get_pnl,
        router::profile::handler::get_position,
        router::profile::handler::get_token_created,
        router::profile::handler::get_swap_history,
        router::search::handler::search_token,
        router::thread::handler::create_thread,
        router::thread::handler::like_thread,
        router::thread::handler::unlike_thread,
        router::thread::handler::get_thread_like_by_account,
        router::token::handler::get_token,
        router::chart::handler::get_chart,
        router::order::handler::get_creation_time_order,
        router::order::handler::get_market_cap_order,
        router::order::handler::get_latest_trade_order,
        router::order::handler::get_reply_count_order,
        router::order::handler::get_latest_reply_order,
        router::follow::handler::add_follow,
        router::follow::handler::remove_follow,
        router::follow::handler::check_follow,
        router::follow::handler::get_followers,
        router::follow::handler::get_followings,
        
    ),
    components(
        schemas(
            // Common
            types::common::info::TokenInfo,
            types::common::info::AccountInfo,
            types::common::pagination::PaginationParams,
            types::common::identifier::Identifier,
            // Auth
            types::auth::AuthNonceRequest,
            types::auth::AuthNonceResponse,
            types::auth::AuthSessionRequest,
            types::auth::AuthSessionResponse,
            
            // Account
            types::account::Account,
            types::account::AccountResponse,
            types::account::UpdateAccountRequest,
            types::account::UpdateAccountFormData,
            types::account::Mutual,
            types::account::MutualFriend,
            
            // Token
            types::token::Token,
            types::token::TokenResponse,
            types::token::create_token::TokenCreated,
            types::token::create_token::TokenCreatedResponse,
            types::token::order::TokenOrderType,
            types::token::order::OrderTokenInfo,
            types::token::order::OrderToken,
            types::token::order::OrderMessage,
            types::token::order::SearchResponse,

            //Trading
            types::trading::chart::Chart,
            types::trading::chart::ChartResponse,
            types::trading::pnl::PNLResponse,
            types::trading::pnl::PeriodPnL,
            types::trading::pnl::BestTrade,
            types::trading::position::Position,
            types::trading::position::PositionResponse,
            types::trading::swap_history::Swap,
            types::trading::swap_history::SwapResponse,

            // Social
            types::social::follow::Follow,
            types::social::follow::FollowResponse,
            types::social::follow::UpdateFollowRequest,
            types::social::follow::UpdateFollowResponse,
            types::social::follow::FollowsResponse,
            types::social::follow::FollowResponse,
            types::social::follow::CheckFollowResponse,
        
            // Thread
            types::social::thread::Thread,
            types::social::thread::ThreadLike,
            types::social::thread::CreateThreadRequest,
            types::social::thread::CreateThreadFormData,
            types::social::thread::ThreadRequest,
            types::social::thread::ThreadResponse,
            
        )
    ),
    tags(
        (name="Auth",description = "Authentication endpoints"),
        (name="Account",description="Account management endpoints"),
        (name="Follow",description="Follow management endpoints"),
        (name="Thread",description="Thread management endpoints"),
        (name="Token",description="Token management endpoints"),
        (name="Profile",description="Profile management endpoints"),
        (name="Search",description="Search endpoints"),
        (name="Chart",description="Chart endpoints"),
        (name="Order",description="Order endpoints"),
        
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
    let port = args.port
        .map(|p| p.to_string())
        .or_else(|| std::env::var("HTTP_PORT").ok())
        .unwrap_or_else(|| "8000".to_string());

    info!("Server will start on {}:{}", ip, port);

    let app_state = AppState::new().await;
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(100)
            .burst_size(10)
            .use_headers()
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .unwrap(),
    );

    let cookie_manager_layer = CookieManagerLayer::new();
    let root = Router::new().route("/", get(|| async { "Hello, World!" }));
    let app = Router::new()
        .merge(root)
        .merge(auth::router(app_state.clone()))
        .merge(account::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(thread::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(token::router())
        .merge(search::router())
        .merge(chart::router())
        .merge(profile::router())
        .merge(order::router())
        .merge(follow::router(app_state.clone()))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_timeout_error))
                .timeout(Duration::from_secs(5)),
        )
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

async fn handle_timeout_error(
    // `Method` and `Uri` are extractors so they can be used here
    method: Method,
    uri: Uri,
    // the last argument must be the error itself
    err: BoxError,
) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("`{method} {uri}` failed with {err}"),
    )
}
