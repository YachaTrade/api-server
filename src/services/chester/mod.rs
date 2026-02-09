use std::sync::Arc;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use crate::{
    controllers::chester::ChesterController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::chester::ChesterRewardsResponse,
};

const CACHE_TTL: Duration = Duration::from_secs(60);

static REWARDS_CACHE: Lazy<RwLock<Option<(ChesterRewardsResponse, Instant)>>> =
    Lazy::new(|| RwLock::new(None));

pub struct ChesterService {
    postgres: Arc<PostgresDatabase>,
}

impl ChesterService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_rewards(&self) -> Result<ChesterRewardsResponse, AppError> {
        // Check cache
        {
            let cache = REWARDS_CACHE.read().await;
            if let Some((ref data, ref ts)) = *cache {
                if ts.elapsed() < CACHE_TTL {
                    return Ok(data.clone());
                }
            }
        }

        // Cache miss → call controller
        let controller = ChesterController::new(self.postgres.clone());
        let response = controller
            .get_rewards()
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        // Update cache
        {
            let mut cache = REWARDS_CACHE.write().await;
            *cache = Some((response.clone(), Instant::now()));
        }

        Ok(response)
    }
}
