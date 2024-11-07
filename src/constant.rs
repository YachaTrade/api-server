use lazy_static::lazy_static;

lazy_static! {
    pub static ref EXPIRATION_SESSION_KEY: u64 = 86_400; // 24 * 60 * 60 = 1Day
    pub static ref NONCE_EXPIRATION:u64 = 300; //5 minutes
}
