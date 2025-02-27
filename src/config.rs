use lazy_static::lazy_static;

lazy_static! {
    pub static ref EXPIRATION_SESSION_KEY: u64 = 86_400; // 24 * 60 * 60 = 1Day
    pub static ref NONCE_EXPIRATION:u64 = 300; //5 minutes

    pub static ref ORDER_EXPIRATION:u64 = 1500; //miliseconds
    pub static ref SEARCH_EXPIRATION:u64 = 30_000; //30 secons in milliseconds
    pub static ref TOKEN_EXPIRATION:u64 = 1500; //miliseconds

    pub static ref TOP_POINT_EXPIRATION:u64 = 300_000; // 5 minutes in milliseconds
    pub static ref MISSION_EXPIRATION:u64 = 1500;// 5 minutes in milliseconds
    pub static ref ACCOUNT_POINT_EXPIRATION: u64 = 60_000; // 1 minute in milliseconds
    pub static ref REFERRAL_CODE_EXPIRATION:u64 = 10_000; // 10 seconds in milliseconds
    pub static ref REFERRAL_CHILD_COUNT_EXPIRATION:u64 = 300_000; // 5 minutes in milliseconds
    pub static ref INVITED_CREATE_EXPIRATION:u64 = 300_000; // 5 minutes in milliseconds
    pub static ref PNL_EXPIRATION:u64 = 60_000; // 1 minute in milliseconds
    pub static ref POSITION_EXPIRATION:u64 = 60_000; // 1 minutes in milliseconds

    pub static ref GET_TOKEN_RESPONSE_EXPIRATION: u64 = 600_000; // 10 minutes in milliseconds

}
