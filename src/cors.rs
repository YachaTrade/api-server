use axum::http::{
    self, HeaderValue,
    header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE},
};
use std::{env, time::Duration};
use tower_http::cors::{AllowOrigin, CorsLayer};

pub fn get_cors() -> CorsLayer {
    // Allow CORS
    // From: https://github.com/MystenLabs/sui/blob/13df03f2fad0e80714b596f55b04e0b7cea37449/crates/sui-faucet/src/main.rs#L85
    // License: Apache-2.0
    let origins = ["https://nad.fun".parse::<HeaderValue>().unwrap(),
        "https://nadapp.net".parse::<HeaderValue>().unwrap(),
        "https://symphony.io".parse::<HeaderValue>().unwrap()];

    let is_dev = env::var("ENVIRONMENT").map(|e| e == "DEV").unwrap_or(false);

    

    CorsLayer::new()
        .allow_methods([
            http::Method::GET,
            http::Method::PUT,
            http::Method::POST,
            http::Method::PATCH,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ])
        .allow_credentials(true)
        .allow_headers([AUTHORIZATION, ACCEPT, CONTENT_TYPE, CONTENT_LENGTH])
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _| {
            origin
                .to_str()
                .map(|origin_string| {
                    origins
                        .iter()
                        .any(|allowed_origin| allowed_origin == origin)
                        || origin_string.ends_with(".nad.fun")
                        || origin_string.ends_with(".symphony.io")
                        || (is_dev && origin_string.starts_with("http://localhost:"))
                })
                .unwrap_or(false)
        }))
        .max_age(Duration::from_secs(86400))
}
