use axum::http::{
    self, HeaderValue,
    header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE},
};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};

/// Exact-match allowed origins (apex domains).
const STATIC_ORIGINS: [&str; 4] = [
    "https://nad.fun",
    "https://nadapp.net",
    "https://symphony.io",
    "https://mm-dashboard-six.vercel.app",
];

/// Whether a request `Origin` is allowed. Exact apex match, allowed suffixes
/// (subdomains), CloudFront distributions, or local dev.
/// NOTE: keep in sync with `crate::middleware::is_allowed_origin` (CSRF check) —
/// a CORS-allowed origin still gets rejected on protected routes if the
/// middleware allowlist disagrees.
fn is_origin_allowed(origin: &str) -> bool {
    STATIC_ORIGINS.contains(&origin)
        || origin.ends_with(".nad.fun")
        || origin.ends_with(".nadapp.net")
        || origin.ends_with(".symphony.io")
        || origin.ends_with(".cloudfront.net")
        || origin.starts_with("http://localhost:")
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
            // exact apex
            ("https://nad.fun", true),
            ("https://nadapp.net", true),
            ("https://symphony.io", true),
            ("https://mm-dashboard-six.vercel.app", true),
            // allowed subdomains
            ("https://app.nad.fun", true),
            ("https://x.symphony.io", true),
            ("https://dev-api.nadapp.net", true),
            ("https://www.nadapp.net", true),
            // CloudFront distributions (newly allowed)
            ("https://d111abcdef8.cloudfront.net", true),
            ("https://assets.d111.cloudfront.net", true),
            // local dev
            ("http://localhost:3000", true),
            // rejected
            ("https://evil.com", false),
            ("https://nad.fun.evil.com", false),
            ("https://notcloudfront.net", false),
            ("https://cloudfront.net", false), // apex without subdomain
            ("https://nadapp.net.attacker.com", false),
        ];
        for (origin, expected) in cases {
            assert_eq!(is_origin_allowed(origin), expected, "origin: {origin}");
        }
    }
}
