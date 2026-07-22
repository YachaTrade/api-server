use axum::http::{
    self, HeaderValue,
    header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE},
};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};
use url::Url;

const YACHA_DOMAIN_SUFFIX: &str = ".yacha.trade";

fn is_canonical_origin(origin: &str, url: &Url) -> bool {
    url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && origin == url.origin().ascii_serialization()
}

fn is_yacha_subdomain_origin(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let Some(subdomains) = host.strip_suffix(YACHA_DOMAIN_SUFFIX) else {
        return false;
    };

    url.scheme() == "https"
        && url.port().is_none()
        && !subdomains.is_empty()
        && subdomains.split('.').all(|label| !label.is_empty())
}

pub(crate) fn is_origin_allowed(origin: &str) -> bool {
    let Ok(url) = Url::parse(origin) else {
        return false;
    };

    if !is_canonical_origin(origin, &url) {
        return false;
    }

    is_yacha_subdomain_origin(&url)
        || (url.scheme() == "http" && url.host_str() == Some("localhost") && url.port().is_some())
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
            ("https://dev.yacha.trade", true),
            ("https://api.yacha.trade", true),
            ("https://a.b.yacha.trade", true),
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
            ("http://dev.yacha.trade", false),
            ("https://dev.yacha.trade:443", false),
            ("https://dev.yacha.trade:8443", false),
            ("https://user@dev.yacha.trade", false),
            ("https://dev.yacha.trade/", false),
            ("https://dev.yacha.trade/path", false),
            ("https://dev.yacha.trade?query", false),
            ("https://dev.yacha.trade#fragment", false),
            ("https://.yacha.trade", false),
            ("https://a..yacha.trade", false),
            ("https://dev.yacha.trade.", false),
            ("https://evil-yacha.trade", false),
            ("https://yacha.trade.evil.com", false),
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
