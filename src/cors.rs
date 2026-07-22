use axum::http::{
    self, HeaderValue,
    header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE},
};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};
use url::Url;

const STATIC_ORIGINS: [&str; 1] = ["https://app.yacha.trade"];

pub(crate) fn is_origin_allowed(origin: &str) -> bool {
    if STATIC_ORIGINS.contains(&origin) {
        return true;
    }

    let Ok(url) = Url::parse(origin) else {
        return false;
    };

    url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && origin == url.origin().ascii_serialization()
}

pub fn get_cors() -> CorsLayer {
    // Allow CORS
    // From: https://github.com/MystenLabs/sui/blob/13df03f2fad0e80714b596f55b04e0b7cea37449/crates/sui-faucet/src/main.rs#L85
    // License: Apache-2.0
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
        .allow_origin(AllowOrigin::predicate(
            |origin: &HeaderValue, _| match origin.to_str() {
                Ok(origin_string) => {
                    let allowed = is_origin_allowed(origin_string);
                    if allowed {
                        info!("[CORS] Origin allowed: {}", origin_string);
                    } else {
                        warn!("[CORS] Origin rejected: {}", origin_string);
                    }
                    allowed
                }
                Err(_) => {
                    warn!("[CORS] Invalid origin header: {:?}", origin);
                    false
                }
            },
        ))
        .max_age(Duration::from_secs(86400))
}

#[cfg(test)]
mod tests {
    use super::is_origin_allowed;

    #[test]
    fn origin_allow_rules() {
        let cases = [
            ("https://app.yacha.trade", true),
            ("http://localhost:3000", true),
            ("http://localhost:8090", true),
            ("http://localhost:", false),
            ("http://localhost:abc", false),
            ("http://localhost:65536", false),
            ("http://localhost:3000.evil", false),
            ("http://user@localhost:3000", false),
            ("http://localhost:3000/path", false),
            ("http://localhost:3000?query", false),
            ("http://localhost:3000#fragment", false),
            ("https://yacha.trade", false),
            ("https://api.yacha.trade", false),
            ("https://nad.fun", false),
            ("https://app.nad.fun", false),
            ("https://nadapp.net", false),
            ("https://dev-api.nadapp.net", false),
            ("https://x.symphony.io", false),
            ("https://d111abcdef8.cloudfront.net", false),
            ("https://mm-dashboard-six.vercel.app", false),
            ("https://evil.com", false),
        ];
        for (origin, expected) in cases {
            assert_eq!(is_origin_allowed(origin), expected, "origin: {origin}");
        }
    }
}
