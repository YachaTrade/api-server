use std::sync::Arc;

use crate::db::{postgres::PostgresDatabase, redis::RedisDatabase};

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);

        Self { postgres, redis }
    }
}
