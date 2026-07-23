use lazy_static::lazy_static;
use std::env;

lazy_static! {
    // Session expiration (in milliseconds)
    pub static ref EXPIRATION_SESSION_KEY: u64 = env::var("EXPIRATION_SESSION_KEY")
        .expect("EXPIRATION_SESSION_KEY must be set")
        .parse::<u64>()
        .expect("EXPIRATION_SESSION_KEY must be a valid u64");

    // Redis cache expirations (all in milliseconds)
    pub static ref MESSAGE_EXPIRATION: u64 = env::var("MESSAGE_EXPIRATION")
        .expect("MESSAGE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("MESSAGE_EXPIRATION must be a valid u64");
    pub static ref ORDER_EXPIRATION: u64 = env::var("ORDER_EXPIRATION")
        .expect("ORDER_EXPIRATION must be set")
        .parse::<u64>()
        .expect("ORDER_EXPIRATION must be a valid u64");
    pub static ref SEARCH_EXPIRATION: u64 = env::var("SEARCH_EXPIRATION")
        .expect("SEARCH_EXPIRATION must be set")
        .parse::<u64>()
        .expect("SEARCH_EXPIRATION must be a valid u64");
    pub static ref TOKEN_TRADE_EXPIRATION: u64 = env::var("TOKEN_TRADE_EXPIRATION")
        .expect("TOKEN_TRADE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("TOKEN_TRADE_EXPIRATION must be a valid u64");
    pub static ref TOKEN_CREATED_EXPIRATION: u64 = env::var("TOKEN_CREATED_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5000); // Default: 5000ms
    pub static ref NEW_CONTENT_EXPIRATION: u64 = env::var("NEW_CONTENT_EXPIRATION")
        .expect("NEW_CONTENT_EXPIRATION must be set")
        .parse::<u64>()
        .expect("NEW_CONTENT_EXPIRATION must be a valid u64");

    pub static ref GET_TOKEN_RESPONSE_EXPIRATION: u64 = env::var("GET_TOKEN_RESPONSE_EXPIRATION")
        .expect("GET_TOKEN_RESPONSE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TOKEN_RESPONSE_EXPIRATION must be a valid u64");
    pub static ref GET_TOKEN_METADATA_EXPIRATION: u64 = env::var("GET_TOKEN_METADATA_EXPIRATION")
        .expect("GET_TOKEN_METADATA_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TOKEN_METADATA_EXPIRATION must be a valid u64");
    pub static ref GET_TREND_TOKEN_RESPONSE_EXPIRATION: u64 = env::var("GET_TREND_TOKEN_RESPONSE_EXPIRATION")
        .expect("GET_TREND_TOKEN_RESPONSE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TREND_TOKEN_RESPONSE_EXPIRATION must be a valid u64");

    // PnL Leaderboard cache expiration (default: 5 minutes = 300000ms)
    pub static ref PNL_LEADERBOARD_RESPONSE_EXPIRATION: u64 = env::var("PNL_LEADERBOARD_RESPONSE_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300000);

    pub static ref NSFW_STATUS_EXPIRATION: u64 = env::var("NSFW_STATUS_EXPIRATION")
        .expect("NSFW_STATUS_EXPIRATION must be set")
        .parse::<u64>()
        .expect("NSFW_STATUS_EXPIRATION must be a valid u64");

    pub static ref GECKO_METADATA_EXPIRATION: u64 = env::var("GECKO_METADATA_EXPIRATION")
        .expect("GECKO_METADATA_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GECKO_METADATA_EXPIRATION must be a valid u64");

    // Contract Addresses
    pub static ref RPC_URL: String = env::var("RPC_URL")
        .expect("RPC_URL must be set");

    // Public custom domain used for every object uploaded to R2.
    pub static ref R2_PUBLIC_BASE_URL: String = {
        let raw = env::var("R2_PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "https://storage.yacha.trade/".to_string());
        let trimmed = raw.trim().trim_end_matches('/');
        assert!(!trimmed.is_empty(), "R2_PUBLIC_BASE_URL must not be empty");
        format!("{trimmed}/")
    };

    pub static ref BONDING_CURVE: String = env::var("BONDING_CURVE")
        .expect("BONDING_CURVE must be set");
    pub static ref TOKEN_IMPL: String = env::var("TOKEN_IMPL")
        .expect("TOKEN_IMPL must be set");
    pub static ref METRICS_REPORT_INTERVAL: u64 = env::var("METRICS_REPORT_INTERVAL")
        .expect("METRICS_REPORT_INTERVAL must be set")
        .parse::<u64>()
        .expect("METRICS_REPORT_INTERVAL must be a valid u64");

    // HTTP request timeout configurations (in milliseconds)
    pub static ref HTTP_GET_TIMEOUT_MS: u64 = env::var("HTTP_GET_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10000); // Default: 10 seconds

    pub static ref HTTP_POST_TIMEOUT_MS: u64 = env::var("HTTP_POST_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(15000); // Default: 15 seconds

    // Database timeout configurations (in milliseconds)
    pub static ref REDIS_TIMEOUT_MS: u64 = env::var("REDIS_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5000); // Default: 5 seconds

    pub static ref POSTGRES_TIMEOUT_MS: u64 = env::var("POSTGRES_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10000); // Default: 10 seconds

    // Token address suffix for vanity addresses
    pub static ref VANITY_ADDRESS_SUFFIX: String = env::var("VANITY_ADDRESS_SUFFIX")
        .unwrap_or_else(|_| "7777".to_string());

    // Namespace every Redis key when multiple services share one instance.
    // Empty prefix keeps legacy key names and makes the startup cleanup a no-op.
    pub static ref REDIS_KEY_PREFIX: String = {
        let raw = env::var("REDIS_KEY_PREFIX").unwrap_or_default();
        let trimmed = raw.trim().trim_end_matches(':');
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("{}:", trimmed)
        }
    };
}
