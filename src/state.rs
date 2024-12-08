use std::sync::Arc;

use crate::db::{postgres::PostgresDatabase, redis::RedisDatabase, S3::S3Client};

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
    pub s3_client: Arc<S3Client>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let s3_client = Arc::new(S3Client::new().await);

        Self {
            postgres,
            redis,
            s3_client,
        }
    }
}
