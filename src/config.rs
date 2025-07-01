use lazy_static::lazy_static;

lazy_static! {
    pub static ref EXPIRATION_SESSION_KEY: u64 = 86_400; // 24 * 60 * 60 = 1Day
    pub static ref MESSAGE_EXPIRATION:u64 = 180; //3 minutes

    pub static ref ORDER_EXPIRATION:u64 = 1500; //miliseconds
    pub static ref SEARCH_EXPIRATION:u64 = 15_000; //30 secons in milliseconds
    pub static ref TOKEN_EXPIRATION:u64 = 1500; //miliseconds


    pub static ref TOP_POINT_EXPIRATION:u64 = 60_000; // 1 minutes in milliseconds
    pub static ref MISSION_EXPIRATION:u64 = 5_000; // 5 seconds in milliseconds
    pub static ref ACCOUNT_POINT_EXPIRATION: u64 = 5_000; // 1 minute in milliseconds
    pub static ref REFERRAL_CODE_EXPIRATION:u64 = 10_000; // 10 seconds in milliseconds
    pub static ref REFERRAL_CHILD_COUNT_EXPIRATION:u64 = 60_000; // 1 minutes in milliseconds
    pub static ref INVITED_CREATE_EXPIRATION:u64 = 60_000; // 1 minutes in milliseconds
    pub static ref PNL_EXPIRATION:u64 = 10000; // 10s in milliseconds
    pub static ref HOLD_TOKEN_EXPIRATION:u64 = 10000; // 10s in milliseconds
    pub static ref GET_TOKEN_RESPONSE_EXPIRATION: u64 = 60_000; // 1 minutes in milliseconds
    pub static ref GET_TOKEN_METADATA_EXPIRATION:u64 = 43_200_000; // 12 hours in milliseconds
    pub static ref GET_HYPE_TOKEN_RESPONSE_EXPIRATION: u64 = 10_000; // 10 seconds in milliseconds





}

//@@@@@@@@@@@@@@@@@@@@TREASURY@@@@@@@@@@@@@@@@@@@@@@@
lazy_static! {
    pub static ref GET_DEV_POSITIONS_EXPIRATION: u64 = 5_000; // 5s in milliseconds
    pub static ref GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION: u64 = 5_000; // 5s in milliseconds
    pub static ref GET_ACCOUNT_LOCKS_EXPIRATION: u64 = 15_000; // 15s in milliseconds

    pub static ref GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION: u64 = 15_000; // 15s in milliseconds

    pub static ref GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION: u64 = 5_000; // 5s in milliseconds
}
