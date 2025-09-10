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

    pub static ref NSFW_STATUS_EXPIRATION: u64 = env::var("NSFW_STATUS_EXPIRATION")
        .expect("NSFW_STATUS_EXPIRATION must be set")
        .parse::<u64>()
        .expect("NSFW_STATUS_EXPIRATION must be a valid u64");


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
}
