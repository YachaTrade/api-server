use api_server::{
    cors::get_cors,
    middleware::authenticate_user,
    router::{self, account, auth, bot, follow, hype, management, metadata, new_content, order, profile, search, token, trade},
    state::AppState,
    types,
};

use std::{
    env, net::{IpAddr, SocketAddr}, str::FromStr, time::Duration
};

use anyhow::Result;
use axum::{
    http::{Method, StatusCode}, middleware as axum_middleware, response::IntoResponse, routing::get, Router
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use axum::http::Request;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use clap::Parser;

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
        // router::account::handler::disconnect_x,
        router::account::handler::get_x_handle,
        router::account::handler::register_wallet,
        router::account::handler::get_wallet,
        
        // ----------------Profile----------------
        router::profile::handler::get_profile,
        router::profile::handler::get_hold_token,
        router::profile::handler::get_token_created,
        router::profile::handler::get_swap_history,

        // ----------------Search----------------
        router::search::handler::search,

        // ----------------Token----------------
        router::token::handler::get_token,
        router::token::handler::get_token_metadata,

        // ----------------Hype----------------
        router::hype::handler::get_hype_token,
        router::hype::handler::get_hype_point,
        router::hype::handler::get_hype_epoch,
        router::hype::handler::get_hype_vote_history,
        router::hype::handler::get_hype_point_history,
        router::hype::handler::get_hype_reward_history,
        router::hype::handler::vote,

        // ----------------Trade----------------
        router::trade::handler::get_swap_history,
        router::trade::handler::get_market,
        router::trade::handler::get_prices,
        router::trade::handler::get_price,
        router::trade::handler::get_holder,
        router::trade::handler::get_management_history,
        router::trade::handler::get_metrics,
        router::trade::handler::get_metrics_batch,

        // ----------------Search----------------
        router::search::handler::search,

        // ----------------Order----------------
        router::order::handler::get_creation_time_order,
        router::order::handler::get_market_cap_order,
        router::order::handler::get_latest_trade_order,
        router::order::handler::get_verified_order,

        // ----------------Follow----------------
        router::follow::handler::add_follow,
        router::follow::handler::remove_follow,
        router::follow::handler::check_follow,
        router::follow::handler::get_followers,
        router::follow::handler::get_followings,  


 
        // ----------------Management----------------
        router::management::handler::get_dev_positions,
        router::management::handler::get_holding_token_management,
        router::management::handler::get_account_locks,
        router::management::handler::get_account_withdrawable_lock,

        // ----------------New Content----------------
        router::new_content::handler::get_new_content,

        // ----------------Metadata----------------
        router::metadata::handler::upload_image,
        router::metadata::handler::upload_metadata,

    ),
    components(
        schemas(
            // Common
            types::common::info::TokenInfo,
            types::common::info::TokenInfoWithCreatedAtAndDescription,
            types::common::info::AccountInfo,
            types::common::info::AccountInfoWithX,
            types::common::info::MarketInfo,
            types::common::info::PositionInfo,
            types::common::info::PositionTokenInfo,
            types::common::info::XInfo,
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
            types::account::Mutual,
            types::account::MutualFriend,
            types::account::x::ConnectXRequest,
            types::account::x::ConnectedXAccountResponse,
            // types::account::x::DisconnectXRequest,
            // types::account::x::DisconnectedXAccountResponse,
            types::account::x::GetXHandleResponse,
            types::account::wallet::RegisterWalletRequest,
            types::account::wallet::AccountWalletResponse,
            types::account::wallet::Wallet,

            // Token
            types::token::TokenWithAccountInfo,
            types::token::TokenResponse,
            types::token::create_token::TokenCreated,
            types::token::create_token::TokenCreatedResponse,
            types::token::order::TokenOrderType,
            types::token::order::OrderTokenInfo,
            types::token::order::OrderToken,
            types::token::order::OrderMessage,
            types::token::metadata::TokenMetadata,
            types::token::metadata::TokenMetadataResponse,
            
            // Metadata
            types::metadata::UploadImageMultipart,
            types::metadata::UploadImageResponse,
            types::metadata::UploadMetadataRequest,
            types::metadata::UploadMetadataResponse,
            types::metadata::TokenMetadata,
            
            //Hype
            types::hype::HypeToken,
            types::hype::HypeTokenResponse,
            types::hype::HypeInfo,
            types::hype::HypePointResponse,
            types::hype::HypeEpochResponse,
            types::hype::HypeVoteHistory,
            types::hype::HypeVoteHistoryResponse,
            types::hype::HypePointRecord,
            types::hype::HypePointRecordResponse,
            types::hype::HypeReward,
            types::hype::HypeRewardHistoryResponse,
            types::hype::HypeVoteRequest,
            types::hype::HypeVoteResponse,

            //Trading
            types::trading::chart::Chart,
            types::trading::chart::ChartResponse,

            types::trading::position::Position,
            types::trading::position::PositionResponse,
            types::trading::position::TokenHolder,
            types::trading::position::TokenHolderResponse,
            types::trading::position::HoldToken,
            types::trading::position::HoldTokenResponse,
            types::trading::swap_history::PositionSwap,
            types::trading::swap_history::PositionSwapResponse,
            types::trading::swap_history::TokenSwap,
            types::trading::swap_history::TokenSwapResponse,
            types::trading::market::Market,
            types::trading::price::PriceResponse,
            types::trading::chart::BarResponse,
            types::trading::chart::GetBarsRequest,
            types::trading::metrics::TimeFrame,
            types::trading::metrics::TokenTradingMetrics,
            types::trading::metrics::TokenTradingMetricsBatch,


            //Search
            types::search::SearchToken,
            types::search::SearchTokenResponse,
            types::search::SearchAccount,
            types::search::SearchAccountResponse,
            types::search::SearchResponse,
            // Social
            types::social::follow::Follow,
            types::social::follow::FollowResponse,
            types::social::follow::UpdateFollowRequest,
            types::social::follow::UpdateFollowResponse,
            types::social::follow::FollowsResponse,
            types::social::follow::FollowResponse,
            types::social::follow::CheckFollowResponse,
        
   
           
           //Management
           types::management::DevPositionsResponse,
           types::management::DevPosition,
           types::management::HoldingTokenManagementResponse,
           types::management::TokenManagement,
           types::management::TokenLockResponse,
           types::management::TokenLock,
           types::management::UnlockInfo,
           types::management::WithdrawableLockResponse,
           types::management::WithdrawableLock,
           types::management::ManagementHistoryResponse,
           types::management::ManagementHistory,
           types::management::ManagementHistoryQuery,

           // New Content
           types::new_content::NewContentResponse,
           types::new_content::NewSwapMessage,
           types::new_content::NewTokenMessage,
           
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
        (name="Bot",description="Bot endpoints"),
        (name="Token Management",description="Token Management endpoints"),
        (name="New Content",description="New Content endpoints"),
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

    info!("Server will start on {}:{} - v2 deployment test", ip, port);
    
    let app_state = AppState::new().await;
    
    let cookie_manager_layer = CookieManagerLayer::new();
    let root = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/health", get(health_check));
    let app = Router::new()
        .merge(root)
        .merge(auth::router(app_state.clone()))
        .merge(account::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(token::router())
        .merge(search::router())
        .merge(trade::router())
        .merge(profile::router())
        .merge(order::router())
        .merge(hype::router(app_state.clone()))
        .merge(follow::router(app_state.clone()))
        .merge(bot::router())
        .merge(management::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(new_content::router())
        .merge(metadata::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
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

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}


// 메서드별 타임아웃 미들웨어
async fn method_based_timeout(
    method: Method,
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let timeout_duration = match method {
        Method::GET => Duration::from_millis(1000),
        _ => Duration::from_millis(2000), // POST, PUT, DELETE 등은 3초
    };
    
    match tokio::time::timeout(timeout_duration, next.run(req)).await {
        Ok(response) => Ok(response),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}
