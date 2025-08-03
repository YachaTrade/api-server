use axum::http::{
    self, HeaderValue,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use std::{env, time::Duration};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

pub fn get_cors() -> CorsLayer {
    // Allow CORS
    // From: https://github.com/MystenLabs/sui/blob/13df03f2fad0e80714b596f55b04e0b7cea37449/crates/sui-faucet/src/main.rs#L85
    // License: Apache-2.0
    let mut origins = vec![
        "https://nad.fun".parse::<HeaderValue>().unwrap(),
        "https://nadapp.net".parse::<HeaderValue>().unwrap(),
    ];
    // Get the `ENVIROMENT` variable and if it is `development` then add `http://localhost:3000`
    // to the `origins` array.
    let environment = env::var("ENVIRONMENT").expect("ENVIRONMENT must be set");
    {
        if environment == "DEV" {
            let allow_cors_port = env::var("ALLOW_CORS_PORT").expect("ALLOW_CORS_PORT must be set");
            if let Ok(localhost_origin) = format!("http://localhost:{}", allow_cors_port).parse() {
                origins.push(localhost_origin);
            }
            if let Ok(test_client) = env::var("CORS_ALLOWED_ORIGINS")
                .expect("CORS_ALLOWED_ORIGINS must be set")
                .parse()
            {
                info!("CORS_ALLOWED_ORIGINS: {:?}", test_client);
                origins.push(test_client);
            }
        }
    }

    let cors = CorsLayer::new()
        .allow_methods([
            http::Method::GET,
            http::Method::PUT,
            http::Method::POST,
            http::Method::PATCH,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ])
        .allow_credentials(true)
        .allow_headers([AUTHORIZATION, ACCEPT, CONTENT_TYPE])
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _| {
            origin
                .to_str()
                .map(|origin_string| {
                    origins
                        .iter()
                        .any(|allowed_origin| allowed_origin == origin)
                        || origin_string.ends_with(".nad.fun")
                })
                .unwrap_or(false)
        }))
        .max_age(Duration::from_secs(86400));

    cors
}
