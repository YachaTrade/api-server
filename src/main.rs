use api_server::{
    config::{HTTP_GET_TIMEOUT_MS, HTTP_POST_TIMEOUT_MS, REDIS_KEY_PREFIX},
    cors::get_cors,
    router::{
        self, account, agent, api_key, auth, chester, cms, dex, health, hype, leaderboard,
        metadata, metrics, new_event, order, profile, quote_token, raffle, search, terminal, token,
        trade, trend, vault,
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
        router::profile::handler::get_gift_fee,
        router::profile::handler::get_swap_history,
        router::profile::handler::get_point_history,

        // ----------------Search----------------
        router::search::handler::search,

        // ----------------Token----------------
        router::token::handler::get_token,
        router::token::handler::get_token_metadata,
        router::token::handler::salt,

        // ----------------Vault----------------
        router::vault::handler::get_token_vaults,

        // ----------------Dex----------------
        router::dex::positions::get_positions,
        router::dex::pool::get_pool,
        router::dex::tokens::get_tokens,
        router::dex::reserves::get_reserves,

        // ----------------QuoteToken----------------
        router::quote_token::handler::list_quote_tokens,

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

        // ----------------Chester----------------
        router::chester::handler::get_volume,
        router::chester::handler::get_round,
        router::chester::handler::get_rewards,
        router::chester::handler::get_swap_history,
        router::chester::handler::get_box_rewards,
        router::chester::handler::get_reward_history,

        // ----------------Raffle----------------
        router::raffle::handler::get_eligible,
        router::raffle::handler::check_raffle,
        router::raffle::handler::get_round,

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

        // ----------------Leaderboard----------------
        router::leaderboard::handler::get_hype_point_leaderboard,
        router::leaderboard::handler::get_pnl_leaderboard,

        // ----------------CMS----------------
        router::cms::handler::set_nsfw,
        router::cms::handler::insert_trend,
        router::cms::handler::update_metadata,
        router::cms::handler::upload_dex_token_image,
        router::cms::handler::upsert_whitelist_token,
        router::cms::handler::list_whitelist_token,

        // ----------------CMS Analytics----------------
        router::cms::analytics::handler::get_churned_users,
        router::cms::analytics::handler::get_active_users,
        router::cms::analytics::handler::get_new_users,
        router::cms::analytics::handler::get_user_roi,
        router::cms::analytics::handler::get_chester_retention,
        router::cms::analytics::handler::get_creator_fee,

        // ----------------Agent----------------
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
            // Common
            types::common::info::TokenInfo,
            types::common::info::AccountInfo,
            types::common::info::MarketInfo,
            types::common::info::MarketType,
            types::common::info::QuoteInfo,
            types::common::info::FeeInfo,
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

            // Vault
            types::vault::VaultType,
            types::vault::TokenVaultsResponse,
            types::vault::VaultEntry,
            types::vault::VaultStats,
            types::vault::BurnStats,
            types::vault::LpStats,
            types::vault::CreatorFeeStats,
            types::vault::GiftStats,
            types::vault::EmptyStats,

            // Dex
            types::dex::position::LpPositionsResponse,
            types::dex::position::LpPositionEntry,
            types::dex::position::LpPositionTokenSide,
            types::dex::pool::PoolDetailResponse,
            types::dex::pool::PoolTokenSide,
            types::dex::pool::FeeConfigInfo,
            types::dex::pool_info::PoolInfo,
            types::dex::tokens::DexTokenListResponse,
            types::dex::tokens::DexTokenEntry,
            types::dex::tokens::DexTokenType,
            types::dex::reserves::ReservesResponse,

            // QuoteToken
            types::quote_token::QuoteTokensResponse,

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
            types::profile::GiftFeeTokensResponse,
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

            // Chester
            types::chester::ChesterVolumeResponse,
            types::chester::ChesterInfoResponse,
            types::chester::ChesterRewardItem,
            types::chester::ChesterRewardsResponse,
            types::chester::ChesterBoxRewardItem,
            types::chester::ChesterBoxRewardsResponse,
            types::chester::ChesterBoxRewardsQuery,
            types::chester::RewardHistoryItem,
            types::chester::ChesterRewardHistoryResponse,

            // Raffle
            types::raffle::RaffleRoundResponse,
            types::raffle::RaffleStatusResponse,
            types::raffle::RaffleCheckQuery,
            types::raffle::RaffleCheckResponse,
            types::raffle::RaffleRound,
            types::raffle::RafflePrizes,

            // Trend
            types::trend::TrendToken,
            types::trend::TrendResponse,
            types::trend::TrendRequest,
            types::trend::TrendActionResponse,

            // Leaderboard
            types::leaderboard::HypePointLeaderboardEntry,
            types::leaderboard::HypePointLeaderboardResponse,
            types::leaderboard::LeaderboardQuery,
            types::leaderboard::Pnl,
            types::leaderboard::PnlLeaderboardEntry,
            types::leaderboard::PnlLeaderboardResponse,

            // CMS
            types::cms::SetNsfwRequest,
            types::cms::InsertTrendRequest,
            types::cms::CmsActionResponse,
            types::cms::UpdateMetadataRequest,
            types::cms::UpdateMetadataResponse,
            types::cms::DexTokenImageResponse,
            types::cms::WhitelistTokenEntry,
            types::cms::WhitelistTokenListResponse,
            router::cms::handler::UpdateMetadataMultipart,
            router::cms::handler::UploadDexTokenImageMultipart,
            router::cms::handler::UpsertWhitelistTokenMultipart,

            // CMS Analytics
            types::cms::analytics::TopHeldToken,
            types::cms::analytics::UserActivityResponse,
            types::cms::analytics::NewUsersResponse,
            types::cms::analytics::UserRoiResponse,
            types::cms::analytics::ChesterRetentionRound,
            types::cms::analytics::ChesterRetentionResponse,
            types::cms::analytics::CreatorFeeResponse,

        )
    ),
    tags(
        (name="Auth",description = "Authentication endpoints"),
        (name="Account",description="Account management endpoints"),
        (name="Follow",description="Follow management endpoints"),
        (name="Token",description="Token management endpoints"),
        (name="Vault",description="V2 token fee vault endpoints (Buyback & Burn / LP / Creator / Gift)"),
        (name="Dex", description="V2 DEX LP positions, pools, and token list"),
        (name="QuoteToken",description="Quote token endpoints"),
        (name="Profile",description="Profile management endpoints"),
        (name="Search",description="Search endpoints"),
        (name="Order",description="Order endpoints"),
        (name="Hype",description="Hype Token endpoints"),
        (name="New Event",description="New Event endpoints"),
        (name="Chester",description="Chester round endpoints"),
        (name="Raffle",description="Raffle endpoints"),
        (name="Metadata",description="Metadata upload endpoints"),
        (name="Terminal",description="Gecko Terminal API endpoints"),
        (name="Trend",description="Trend token endpoints"),
        (name="Leaderboard",description="Leaderboard endpoints"),
        (name="CMS",description="CMS admin endpoints"),
        (name="CMS Analytics",description="CMS analytics dashboard endpoints"),
        (name="Agent",description="Agent API endpoints for AI integrations"),
    ),
    security(
        ("session_cookie" = [])
    )
)]
pub struct ApiDoc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port number for the server. If unset, falls back to PORT env, then
    /// HTTP_PORT env. If none of the three are set, the server panics on
    /// startup — no silent default. Set PORT explicitly per environment.
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
    // 우선순위: 1) --port CLI 2) PORT env 3) HTTP_PORT env (legacy 호환).
    // 셋 다 없으면 panic — 무음 fallback 없음.
    let port = args
        .port
        .map(|p| p.to_string())
        .or_else(|| env::var("PORT").ok())
        .or_else(|| env::var("HTTP_PORT").ok())
        .expect(
            "PORT must be set: pass --port, or set the PORT (or HTTP_PORT) env var \
             — likely missing from .env",
        );

    info!("Server will start on {}:{} - v2 deployment test", ip, port);

    let app_state = AppState::new().await;
    info!("AppState initialized");

    // Redis startup flush — prefix-scoped SCAN + DEL only.
    // No-op when REDIS_KEY_PREFIX is empty (we never run FLUSHALL).
    if !REDIS_KEY_PREFIX.is_empty() {
        info!(
            "Flushing Redis keys matching prefix {}* ...",
            REDIS_KEY_PREFIX.as_str()
        );
    }
    if let Err(e) = app_state.redis.flush_all().await {
        warn!("Failed to flush Redis: {}", e);
    }

    // Background task: API Key 사용량 주기적 DB 동기화 (5분마다)
    let sync_state = app_state.clone();
    tokio::spawn(async move {
        use api_server::services::api_key::sync_api_key_usage_to_db;
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
        loop {
            interval.tick().await;
            match sync_api_key_usage_to_db(&sync_state.postgres, &sync_state.redis).await {
                Ok(count) => {
                    if count > 0 {
                        info!("API key usage synced to DB: {} keys", count);
                    }
                }
                Err(e) => {
                    warn!("Failed to sync API key usage: {:?}", e);
                }
            }
        }
    });

    let cookie_manager_layer = CookieManagerLayer::new();
    let root = Router::new().route("/", get(|| async { "Hello, World!" }));
    let app = Router::new()
        .merge(root)
        .merge(health::router())
        .merge(auth::router(app_state.clone()))
        .merge(account::router(app_state.clone()))
        .merge(raffle::router(app_state.clone()))
        .merge(token::router())
        .merge(vault::router())
        .merge(dex::router())
        .merge(quote_token::router())
        .merge(search::router())
        .merge(trade::router())
        .merge(profile::router(app_state.clone()))
        .merge(order::router())
        .merge(hype::router(app_state.clone()))
        // .merge(bot::router()) // bot 모듈이 존재하지 않음
        .merge(chester::router(app_state.clone()))
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
        .layer(DefaultBodyLimit::max(100_000)) // 100KB global limit (image upload has separate 5MB limit)
        .layer(axum_middleware::from_fn(method_based_timeout))
        .layer(axum_middleware::from_fn_with_state(
            app_state.clone(),
            api_server::middleware::api_key_gate,
        ))
        .layer(ServiceBuilder::new().layer(get_cors()).into_inner())
        .layer(cookie_manager_layer)
        // .layer(GovernorLayer {
        //     config: governor_conf,
        // })
        .with_state(app_state)
        .fallback(handler_404);

    let ip_addr = IpAddr::from_str(ip.as_str())
        .map_err(|e| anyhow::anyhow!("Invalid IP address '{}': {}", ip, e))?;
    let port_num: u16 = port
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid port '{}': {}", port, e))?;
    let addr = SocketAddr::from((ip_addr, port_num));

    info!("Listening on {} Server port{}", addr, port);

    axum_server::bind(addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| anyhow::anyhow!("Server failed to start: {}", e))?;
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
    let is_upload_endpoint = path.starts_with("/metadata/image")
        || path.starts_with("/metadata/metadata")
        || path.starts_with("/agent/token/image")
        || path.starts_with("/cms/dex-token/image");

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

#[cfg(test)]
mod openapi_tests {
    use super::ApiDoc;
    use utoipa::OpenApi;

    /// token_type의 가능한 값이 swagger(OpenAPI)에 enum으로 명시되는지 검증.
    #[test]
    fn dex_token_type_enum_documented_in_openapi() {
        let spec = ApiDoc::openapi();
        let json = serde_json::to_value(&spec).unwrap();
        let schema = &json["components"]["schemas"]["DexTokenType"];
        assert!(!schema.is_null(), "DexTokenType schema가 OpenAPI components에 등록돼야 함");
        let vals: Vec<&str> = schema["enum"]
            .as_array()
            .expect("DexTokenType는 string enum이어야 함")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            vals,
            vec!["whitelist", "nadfun_v2", "nadfun_v1", "external"],
            "token_type 가능한 값이 swagger에 명시돼야 함"
        );
    }
}
