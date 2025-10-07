use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::warn;

use crate::{db::postgres::PostgresDatabase, measure_postgres, types::metadata::TokenMetadata};

pub struct MetadataController {
    pub db: Arc<PostgresDatabase>,
}

impl MetadataController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MetadataController { db }
    }

    pub async fn save_token_metadata(
        &self,
        metadata: &TokenMetadata,
        metadata_url: &str,
    ) -> Result<()> {
        let start_time = Instant::now();

        let query = sqlx::query!(
            r#"
            INSERT INTO token_metadata (metadata_url, name, symbol, description, image_url, website, twitter, telegram, is_nsfw)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            metadata_url,
            metadata.name,
            metadata.symbol,
            metadata.description,
            metadata.image_uri,
            metadata.website,
            metadata.twitter,
            metadata.telegram,
            metadata.is_nsfw
        );

        measure_postgres!(
            "metadata.save_token_metadata",
            query.execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow::anyhow!("Failed to save token metadata. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        if elapsed > Duration::from_millis(100) {
            warn!(
                "save_token_metadata query slow performance: {:?} for name: {}, symbol: {}",
                elapsed, metadata.name, metadata.symbol
            );
        }

        Ok(())
    }
}
