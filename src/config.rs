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
    pub static ref GET_HYPE_TOKEN_RESPONSE_EXPIRATION: u64 = env::var("GET_HYPE_TOKEN_RESPONSE_EXPIRATION")
        .expect("GET_HYPE_TOKEN_RESPONSE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_HYPE_TOKEN_RESPONSE_EXPIRATION must be a valid u64");
    pub static ref GET_TREND_TOKEN_RESPONSE_EXPIRATION: u64 = env::var("GET_TREND_TOKEN_RESPONSE_EXPIRATION")
        .expect("GET_TREND_TOKEN_RESPONSE_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TREND_TOKEN_RESPONSE_EXPIRATION must be a valid u64");
    pub static ref GET_TOTAL_HYPE_POINT_EXPIRATION: u64 = env::var("GET_TOTAL_HYPE_POINT_EXPIRATION")
        .expect("GET_TOTAL_HYPE_POINT_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TOTAL_HYPE_POINT_EXPIRATION must be a valid u64");
    pub static ref GET_COMMUNITY_TREASURY_EXPIRATION: u64 = env::var("GET_COMMUNITY_TREASURY_EXPIRATION")
        .expect("GET_COMMUNITY_TREASURY_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_COMMUNITY_TREASURY_EXPIRATION must be a valid u64");
    pub static ref GET_REWARD_ADD_HISTORY_EXPIRATION: u64 = env::var("GET_REWARD_ADD_HISTORY_EXPIRATION")
        .expect("GET_REWARD_ADD_HISTORY_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_REWARD_ADD_HISTORY_EXPIRATION must be a valid u64");

    // Leaderboard cache expiration (default: 10 seconds = 10000ms)
    pub static ref HYPE_LEADERBOARD_RESPONSE_EXPIRATION: u64 = env::var("HYPE_LEADERBOARD_RESPONSE_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10000);

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

    // Treasury related expirations
    pub static ref GET_DEV_POSITIONS_EXPIRATION: u64 = env::var("GET_DEV_POSITIONS_EXPIRATION")
        .expect("GET_DEV_POSITIONS_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_DEV_POSITIONS_EXPIRATION must be a valid u64");
    pub static ref GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION: u64 = env::var("GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION")
        .expect("GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION must be a valid u64");
    pub static ref GET_ACCOUNT_LOCKS_EXPIRATION: u64 = env::var("GET_ACCOUNT_LOCKS_EXPIRATION")
        .expect("GET_ACCOUNT_LOCKS_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_ACCOUNT_LOCKS_EXPIRATION must be a valid u64");
    pub static ref GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION: u64 = env::var("GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION")
        .expect("GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION must be a valid u64");
    pub static ref GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION: u64 = env::var("GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION")
        .expect("GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION must be set")
        .parse::<u64>()
        .expect("GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION must be a valid u64");

    // Contract Addresses
    pub static ref COMMUNITY_TREASURY: String = env::var("COMMUNITY_TREASURY")
        .expect("COMMUNITY_TREASURY must be set");
    pub static ref WMON: String = env::var("WMON")
        .expect("WMON must be set");
    pub static ref RPC_URL: String = env::var("RPC_URL")
        .expect("RPC_URL must be set");

    // GitHub API Token (for higher rate limit: 5000/hour)
    pub static ref GITHUB_TOKEN: String = env::var("GITHUB_TOKEN")
        .expect("GITHUB_TOKEN must be set");

    // Hackathon GitHub data refresh interval (in seconds, default: 3600 = 1 hour)
    pub static ref HACKATHON_REFRESH_INTERVAL_SECS: i64 = env::var("HACKATHON_REFRESH_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3600);

    pub static ref BONDING_CURVE:String = env::var("BONDING_CURVE")
        .expect("BONDING_CURVE must be set");
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
}
