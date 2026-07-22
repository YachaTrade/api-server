pub mod meta;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    pub name: String,
    pub symbol: String,
    pub decimals: i32,
}

#[async_trait]
pub trait TokenMetaSource: Send + Sync {
    async fn token_meta(&self, token_id: &str) -> Option<TokenMeta>;
}
