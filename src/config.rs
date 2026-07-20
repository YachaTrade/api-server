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

    // Vault response cache (default: 30 seconds = 30000ms)
    pub static ref GET_TOKEN_VAULTS_RESPONSE_EXPIRATION: u64 = env::var("GET_TOKEN_VAULTS_RESPONSE_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30000);

    // Quote tokens response cache (default: 5 minutes = 300000ms — quote tokens are essentially static)
    pub static ref GET_QUOTE_TOKENS_RESPONSE_EXPIRATION: u64 = env::var("GET_QUOTE_TOKENS_RESPONSE_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300000);

    // Gift fee tokens response cache (default: 10 seconds = 10000ms)
    pub static ref GET_GIFT_FEE_RESPONSE_EXPIRATION: u64 = env::var("GET_GIFT_FEE_RESPONSE_EXPIRATION")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10000);

    // Dev Post cache expirations (default: 60 seconds = 60000ms)
    pub static ref DEVPOST_RANKING_EXPIRATION: u64 = env::var("DEVPOST_RANKING_EXPIRATION")
        .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60_000);
    pub static ref DEVPOST_TRENDING_EXPIRATION: u64 = env::var("DEVPOST_TRENDING_EXPIRATION")
        .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60_000);
    pub static ref DEVPOST_FEED_EXPIRATION: u64 = env::var("DEVPOST_FEED_EXPIRATION")
        .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60_000);
    pub static ref DEVPOST_DETAIL_EXPIRATION: u64 = env::var("DEVPOST_DETAIL_EXPIRATION")
        .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60_000);

    // Contract Addresses
    pub static ref COMMUNITY_TREASURY: String = env::var("V1_COMMUNITY_TREASURY")
        .expect("V1_COMMUNITY_TREASURY must be set");
    pub static ref WETH: String = env::var("WETH")
        .expect("WETH must be set");
    pub static ref RPC_URL: String = env::var("RPC_URL")
        .expect("RPC_URL must be set");

    // Chester reward token contract addresses
    pub static ref MON_CONTRACT_ADDRESS: String = env::var("MON_CONTRACT_ADDRESS")
        .expect("MON_CONTRACT_ADDRESS must be set");
    pub static ref APR_CONTRACT_ADDRESS: String = env::var("APR_CONTRACT_ADDRESS")
        .expect("APR_CONTRACT_ADDRESS must be set");

    pub static ref V1_BONDING_CURVE: String = env::var("V1_BONDING_CURVE")
        .expect("V1_BONDING_CURVE must be set");
    pub static ref V1_TOKEN_IMPL: String = env::var("V1_TOKEN_IMPL")
        .expect("V1_TOKEN_IMPL must be set");
    pub static ref V2_BONDING_CURVE: String = env::var("V2_BONDING_CURVE")
        .expect("V2_BONDING_CURVE must be set");
    pub static ref V2_TOKEN_IMPL: String = env::var("V2_TOKEN_IMPL")
        .expect("V2_TOKEN_IMPL must be set");
    // Singleton DividendVault address. Identifies the dividend fee-split
    // allocation (creator_fee_allocation.vault_id) for the "Dividend %" card.
    // Optional: empty -> dividend_bps reports 0 (feature degrades gracefully).
    pub static ref V2_DIVIDEND_VAULT: String = env::var("V2_DIVIDEND_VAULT")
        .unwrap_or_default();
    /// Source-token holders to exclude from dividend `recipient_count`: protocol
    /// contracts (vaults, curve, router, factory, managers, …) plus the null and
    /// dead burn sinks. The per-token contract itself and its DEX pool are NOT
    /// fixed addresses, so they are excluded directly in the query. Missing env
    /// vars are skipped, so a partially-configured env (e.g. testnet without
    /// FEE_TO / FOUNDATION_TREASURY) still works.
    pub static ref DIVIDEND_RECIPIENT_BLACKLIST: Vec<String> = {
        const ENV_KEYS: &[&str] = &[
            "V2_TOKEN_IMPL", "V2_PROTOCOL_MANAGER", "V2_TOKEN_REGISTRY", "V2_LP_MANAGER",
            "V2_BONDING_CURVE", "V2_CREATOR_FEE_PROCESSOR", "V2_FEE_COLLECTOR",
            "V2_NAD_FUN_PAIR_IMPL", "V2_NAD_FUN_FACTORY", "V2_NAD_SWAP_ADAPTER",
            "V2_VAULT_REGISTRY", "V2_BURN_VAULT", "V2_LP_VAULT", "V2_CREATOR_FEE_VAULT",
            "V2_GIFT_VAULT", "V2_NAD_FUN_ROUTER", "V2_FEE_TO", "V2_FOUNDATION_TREASURY",
            "V2_DIVIDEND_VAULT",
        ];
        let mut v: Vec<String> = ENV_KEYS
            .iter()
            .filter_map(|k| env::var(k).ok())
            .filter(|s| !s.is_empty())
            .collect();
        // Null + dead burn sinks. Dead is added in both checksum and lowercase
        // forms so exact match works regardless of how the indexer stored it,
        // keeping LOWER() out of the count query.
        v.push("0x0000000000000000000000000000000000000000".to_string());
        v.push("0x000000000000000000000000000000000000dEaD".to_string());
        v.push("0x000000000000000000000000000000000000dead".to_string());
        v
    };
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

    // Global prefix for every Redis key. Empty string = no prefix (legacy
    // behavior). Set when sharing one Redis instance between v1 and v2.
    // The trailing colon is appended automatically: REDIS_KEY_PREFIX="API_V2"
    // → all keys become "API_V2:<original-key>".
    //
    // NOTE: startup flush is keyed off this value. Empty prefix → startup
    // flush is a no-op (we never run FLUSHALL — too dangerous on shared
    // Redis). Non-empty prefix → SCAN+DEL only the keys we own.
    pub static ref PYTH_HERMES_URL: String =
        env::var("PYTH_HERMES_URL").unwrap_or_else(|_| "https://hermes.pyth.network".to_string());

    pub static ref REDIS_KEY_PREFIX: String = {
        let raw = env::var("REDIS_KEY_PREFIX").unwrap_or_default();
        let trimmed = raw.trim().trim_end_matches(':');
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("{}:", trimmed)
        }
    };

    // ---- X (Twitter) hidden-creator verification ----
    pub static ref X_CLIENT_ID: String =
        env::var("X_CLIENT_ID").expect("X_CLIENT_ID must be set");
    /// Confidential (Web App) client → HTTP Basic auth on token exchange.
    /// Empty string ⇒ public (Native) client (PKCE only, no secret).
    pub static ref X_CLIENT_SECRET: String =
        env::var("X_CLIENT_SECRET").unwrap_or_default();
    /// Must EXACTLY match a Callback URI registered in the X app.
    pub static ref X_REDIRECT_URI: String =
        env::var("X_REDIRECT_URI").expect("X_REDIRECT_URI must be set");
    /// Fixed front-end URL to redirect to after a successful callback.
    /// Server-controlled (open-redirect prevention) — never client-supplied.
    pub static ref X_OAUTH_REDIRECT_SUCCESS_URL: String =
        env::var("X_OAUTH_REDIRECT_SUCCESS_URL").expect("X_OAUTH_REDIRECT_SUCCESS_URL must be set");
    /// Fixed front-end URL to redirect to on callback failure.
    pub static ref X_OAUTH_REDIRECT_FAILURE_URL: String =
        env::var("X_OAUTH_REDIRECT_FAILURE_URL").expect("X_OAUTH_REDIRECT_FAILURE_URL must be set");
    /// PKCE state / code_verifier lifetime in Redis (ms). Default 10 min.
    pub static ref X_OAUTH_STATE_TTL_MS: u64 = env::var("X_OAUTH_STATE_TTL_MS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(600_000);
    /// Pending verification lifetime in Redis (ms). Default 30 min.
    pub static ref X_PENDING_TTL_MS: u64 = env::var("X_PENDING_TTL_MS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(1_800_000);
    /// Max number of followed-by handles per pending verification. Default 3.
    pub static ref X_FOLLOWED_BY_MAX: usize = env::var("X_FOLLOWED_BY_MAX")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    /// Minimum follower count required for a followed-by handle to be added;
    /// accounts below this are rejected (`insufficient_followers`). Default 1000.
    pub static ref X_FOLLOWED_BY_MIN_FOLLOWERS: i64 = env::var("X_FOLLOWED_BY_MIN_FOLLOWERS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
}

pub fn validate_devpost_cache_ttls(values: [(&str, u64); 4]) -> Result<(), String> {
    for (name, value) in values {
        if value > 60_000 {
            return Err(format!("{name} must be <= 60000ms, got {value}"));
        }
    }
    Ok(())
}

pub fn validate_current_devpost_cache_ttls() -> Result<(), String> {
    validate_devpost_cache_ttls([
        ("DEVPOST_FEED_EXPIRATION", *DEVPOST_FEED_EXPIRATION),
        ("DEVPOST_DETAIL_EXPIRATION", *DEVPOST_DETAIL_EXPIRATION),
        ("DEVPOST_TRENDING_EXPIRATION", *DEVPOST_TRENDING_EXPIRATION),
        ("DEVPOST_RANKING_EXPIRATION", *DEVPOST_RANKING_EXPIRATION),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devpost_cache_ttl_ceiling_accepts_60000_and_rejects_60001() {
        assert!(
            validate_devpost_cache_ttls([
                ("DEVPOST_FEED_EXPIRATION", 60_000),
                ("DEVPOST_DETAIL_EXPIRATION", 60_000),
                ("DEVPOST_TRENDING_EXPIRATION", 60_000),
                ("DEVPOST_RANKING_EXPIRATION", 60_000),
            ])
            .is_ok()
        );
        let error = validate_devpost_cache_ttls([
            ("DEVPOST_FEED_EXPIRATION", 60_000),
            ("DEVPOST_DETAIL_EXPIRATION", 60_001),
            ("DEVPOST_TRENDING_EXPIRATION", 60_000),
            ("DEVPOST_RANKING_EXPIRATION", 60_000),
        ])
        .unwrap_err();
        assert!(error.contains("DEVPOST_DETAIL_EXPIRATION"), "{error}");
    }

    #[test]
    fn followed_by_max_defaults_to_three() {
        // X_FOLLOWED_BY_MAX is unset in the test env → default 3.
        assert_eq!(*X_FOLLOWED_BY_MAX, 3);
    }

    #[test]
    fn followed_by_min_followers_defaults_to_1000() {
        assert_eq!(*X_FOLLOWED_BY_MIN_FOLLOWERS, 1000);
    }
}
