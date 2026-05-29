use std::sync::Arc;

use crate::db::{postgres::PostgresDatabase, r2::R2Client, redis::RedisDatabase};
use crate::services::gift::config::GiftConfig;
use crate::services::gift::parser::GiftParser;

/// Optional gift runtime: present only when GIFT_* config loads and the parser
/// builds. Missing/invalid config disables gift features without crashing the
/// server (graceful degradation).
#[derive(Clone)]
pub struct GiftRuntime {
    pub config: Arc<GiftConfig>,
    pub parser: Arc<GiftParser>,
}

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
    pub r2: Arc<R2Client>,
    pub gift: Option<Arc<GiftRuntime>>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let r2 = Arc::new(R2Client::new().await);

        let gift = match GiftConfig::load() {
            Ok(cfg) => match GiftParser::new(&cfg) {
                Ok(parser) => {
                    tracing::info!("gift runtime initialized");
                    Some(Arc::new(GiftRuntime {
                        config: Arc::new(cfg),
                        parser: Arc::new(parser),
                    }))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "gift parser init failed; gift features disabled");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "GIFT_* config absent/invalid; gift features disabled");
                None
            }
        };

        Self {
            postgres,
            redis,
            r2,
            gift,
        }
    }
}
