use api_server::{
    cors::get_cors,
    db, env,
    middleware::authenticate_user,
    router::{self, account, auth, balance, search, thread, token},
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
    error_handling::HandleErrorLayer,
    http::{Method, StatusCode, Uri},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::get,
    BoxError, Router,
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, GovernorLayer,
};
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

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
        // router::profile::handler::get_followers,
        // router::profile::handler::get_following,
        router::search::handler::search_token,
        router::thread::handler::create_thread,
        router::thread::handler::like_thread,
        router::thread::handler::unlike_thread,
        router::token::handler::update_token,
    ),
    components(schemas(
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
        // router::profile::handler::FollowersResponse,
        // router::profile::handler::FollowingResponse,
        router::search::handler::SearchResponse,
        router::thread::handler::CreateThreadRequest,
        router::thread::handler::CreateThreadFormData,
        router::thread::handler::ThreadRequest,
        router::thread::handler::ThreadResponse,
        router::token::handler::UpdateTokenRequest,
        router::token::handler::UpdateTokenResponse,
        db::postgres::model::Account,
        db::postgres::model::Token,
        db::postgres::model::Thread,

    )),
    tags(
        (name="Auth",description = "Authentication endpoints"),
        (name="Balance",description="Balance management endpoints"),
        (name="Account",description="Account management endpoints"),
        // (name="Follow",description="Follow management endpoints"),
        (name="Thread",description="Thread management endpoints"),
        (name="Token",description="Token management endpoints"),
        (name="Profile",description="Profile management endpoints"),
    )
)]
pub struct ApiDoc;

#[tokio::main]
async fn main() -> Result<()> {
    info!("Axum server started");
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let ip = env::get_env("IP");
    let port = env::get_env("HTTP_PORT");

    let app_state = AppState::new().await;
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            // 밀리초당 허용되는 요청 수를 설정합니다.
            // 여기서는 300밀리초(0.3초)마다 1개의 요청을 허용합니다.
            // 즉, 초당 약 3.33개의 요청을 허용합니다.
            .per_second(10)
            // 버스트 크기를 설정합니다.
            // 이는 짧은 시간 동안 한 번에 처리할 수 있는 최대 요청 수입니다.
            // 여기서는 최대 300개의 요청을 버스트로 허용합니다.
            .burst_size(10)
            // 클라이언트 식별을 위해 HTTP 헤더를 사용하도록 설정합니다.
            // 이는 X-Forwarded-For 또는 X-Real-IP와 같은 헤더를 통해
            // 클라이언트의 실제 IP를 식별하는 데 유용합니다.
            .use_headers()
            // 요청을 구분하기 위한 키 추출기를 설정합니다.
            // SmartIpKeyExtractor는 클라이언트 IP를 지능적으로 추출하여
            // 프록시나 로드 밸런서 뒤에 있는 실제 클라이언트 IP를 식별합니다.
            // 기본은 PeerIpKeyExtractor reverse proxy = SmartIpKeyExtractor
            .key_extractor(SmartIpKeyExtractor)
            // 설정을 완료하고 GovernorConfig 인스턴스를 생성합니다.
            .finish()
            .unwrap(),
    );

    let cookie_manager_layer = CookieManagerLayer::new();
    let root = Router::new().route("/", get(|| async { "Hello, World!" }));
    let app = Router::new()
        .merge(root)
        .merge(auth::router())
        .merge(account::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(token::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(thread::router().layer(ServiceBuilder::new().layer(
            axum_middleware::from_fn_with_state(app_state.clone(), authenticate_user),
        )))
        .merge(balance::router())
        .merge(search::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_timeout_error))
                .timeout(Duration::from_secs(5)),
        )
        .layer(ServiceBuilder::new().layer(get_cors()).into_inner())
        .layer(cookie_manager_layer)
        .layer(GovernorLayer {
            config: governor_conf,
        })
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
