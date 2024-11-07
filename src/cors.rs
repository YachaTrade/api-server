use axum::http::{
    self,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    HeaderValue,
};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::env;
pub fn get_cors() -> CorsLayer {
    // Allow CORS
    // From: https://github.com/MystenLabs/sui/blob/13df03f2fad0e80714b596f55b04e0b7cea37449/crates/sui-faucet/src/main.rs#L85
    // License: Apache-2.0
    let mut origins = vec!["https://nad.fun".parse::<HeaderValue>().unwrap()];
    // Get the `ENVIROMENT` variable and if it is `development` then add `http://localhost:3000`
    // to the `origins` array.
    let environment = env::get_env("ENVIRONMENT");
    {
        if environment == "development" {
            let allow_cors_port = env::get_env("ALLOW_CORS_PORT");
            if let Ok(localhost_origin) = format!("http://localhost:{}", allow_cors_port).parse() {
                origins.push(localhost_origin);
            }
            if let Ok(test_client) = "https://main.d3j5jzvozo0dgk.amplifyapp.com".parse() {
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
            // http::Method::OPTIONS,
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
