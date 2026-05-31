use std::sync::Arc;

use crate::db::{postgres::PostgresDatabase, r2::R2Client, redis::RedisDatabase};
use crate::services::capricorn::CapricornClient;

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
    pub r2: Arc<R2Client>,
    pub capricorn: Arc<CapricornClient>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let r2 = Arc::new(R2Client::new().await);
        let capricorn = Arc::new(CapricornClient::new(
            std::env::var("CAPRICORN_GRAPHQL_URL").unwrap_or_default(),
        ));

        Self {
            postgres,
            redis,
            r2,
            capricorn,
        }
    }
}
