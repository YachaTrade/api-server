use std::sync::Arc;

use crate::db::{r2::R2Client, postgres::PostgresDatabase, redis::RedisDatabase};

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
    pub r2: Arc<R2Client>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let r2 = Arc::new(R2Client::new().await);

        Self {
            postgres,
            redis,
            r2,
        }
    }
}
