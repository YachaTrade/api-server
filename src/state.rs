use std::sync::Arc;

use crate::db::{aws::S3Client, postgres::PostgresDatabase, redis::RedisDatabase};

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub session_redis: Arc<RedisDatabase>,
    pub trade_redis: Arc<RedisDatabase>,
    pub s3_client: Arc<S3Client>,
}

impl AppState {
    pub async fn new() -> Self {
        let session_redis = Arc::new(RedisDatabase::new_session_pool().await);
        let trade_redis = Arc::new(RedisDatabase::new_trade_pool().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let s3_client = Arc::new(S3Client::new().await);

        Self {
            postgres,
            session_redis,
            trade_redis,
            s3_client,
        }
    }
}
