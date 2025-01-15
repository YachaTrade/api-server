use api_server::{
    cors::get_cors,
    db, env,
    middleware::authenticate_user,
    router::{self, account, auth, balance, chart, order, profile, search, thread, token},
    state::AppState,
    types,
};

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use axum::{
    error_handling::HandleErrorLayer, http::{Method, StatusCode, Uri}, middleware as axum_middleware, response::IntoResponse, routing::get, BoxError, Extension, Router
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, GovernorLayer,
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
        router::balance::handler::get_balance,
        router::account::handler::update_account,
        router::account::handler::add_account_like,
        router::account::handler::remove_account_like,
        router::account::handler::get_account,
        router::profile::handler::get_profile,
        router::profile::handler::get_created_tokens,
        router::profile::handler::get_tokens_held,
        router::profile::handler::get_replies,
        router::profile::handler::get_followers,
        router::profile::handler::get_following,
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
        // router::mint_party::handler::create_mint_party,
        // router::mint_party::handler::update_mint_party,
        // router::mint_party::handler::get_last_join_mint_party,
        // router::mint_party::handler::get_mint_party_list,
        // router::mint_party::handler::get_mint_party_deposit_list,
        // router::mint_party::handler::get_mint_party_balance,
    ),
    components(
        schemas(
            router::auth::handler::AuthNonceRequest,
            router::auth::handler::AuthNonceResponse,
            router::auth::handler::AuthSessionRequest,
            router::auth::handler::AuthSessionResponse,
            types::response::HoldTokenResponse,
            router::account::handler::UpdateAccountRequest,
            router::account::handler::UpdateAccountFormData,
            router::account::handler::AccountResponse,
   
            router::account::handler::AddLikeRequest,
            router::account::handler::RemoveLikeRequest,
            router::profile::handler::ProfileResponse,
            router::profile::handler::HeldTokensResponse,
            router::profile::handler::RepliesResponse,
            router::profile::handler::CreatedTokensResponse,
            router::profile::handler::FollowersResponse,
            router::profile::handler::FollowingResponse,
            
            router::thread::handler::CreateThreadRequest,
            router::thread::handler::CreateThreadFormData,
            router::thread::handler::ThreadRequest,
            router::thread::handler::ThreadResponse,
            router::thread::handler::ThreadLikeResponse,
            router::token::handler::TokenResponse,
            router::chart::handler::ChartResponse,
            router::chart::handler::ChartQuery,
            router::order::handler::OrderMessage,
            // router::mint_party::handler::UpdateMintPartyRequest,
            // router::mint_party::handler::UpdateMintPartyResponse,
            // router::mint_party::handler::MintPartyListResponse,
            // router::mint_party::handler::MintPartyBalanceResponse,
            // router::mint_party::handler::MintPartyDepositListResopnse,
            // router::mint_party::handler::MintPartyQuery,
            // types::order_type::MintPartyOrderType,
            
            // types::response::MintPartyResponse,
            // types::response::MintPartyBalance,
            // types::response::MintPartyDepositList,
            // types::response::MintPartyInfo,
            types::response::AccountInfo,
            types::response::TokenInfoResponse,
            types::response::SearchResponse,
            types::response::TokenInfo,
            types::response::CreateTokenResponse,
            db::postgres::model::Account,
            db::postgres::model::Token,
            db::postgres::model::Thread,
            db::postgres::model::Token,
            db::postgres::model::Chart,
            // db::postgres::model::MintParty
    )),
    tags(
        (name="Auth",description = "Authentication endpoints"),
        (name="Balance",description="Balance management endpoints"),
        (name="Account",description="Account management endpoints"),
        (name="Follow",description="Follow management endpoints"),
        (name="Thread",description="Thread management endpoints"),
        (name="Token",description="Token management endpoints"),
        (name="Profile",description="Profile management endpoints"),
        (name="Search",description="Search endpoints"),
        (name="Chart",description="Chart endpoints"),
        (name="Order",description="Order endpoints"),
        // (name="MintParty", description ="Mint Party management endpoints")
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
    
    let ip = env::get_env("IP");
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
        .merge(balance::router())
        .merge(search::router())
        .merge(chart::router())
        .merge(profile::router())
        .merge(order::router())
        // .merge(mint_party::router(app_state.clone()))
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
