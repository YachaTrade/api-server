use std::sync::Arc;

use crate::db::postgres::{
    model::{Chart, ChartInterval},
    PostgresDatabase,
};

use anyhow::{anyhow, Result};
use tracing::info;

pub struct ChartController {
    pub db: Arc<PostgresDatabase>,
}

impl ChartController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ChartController { db }
    }
    pub async fn get_chart(
        &self,
        token_id: &str,
        interval: ChartInterval,
        pagination: i16,
    ) -> Result<Vec<Chart>> {
        let chart_interval: i16 = interval.into();
        let pagination = if pagination <= 0 { 1 } else { pagination };
        let offset = ((pagination - 1) as i64) * 300;
        info!(
            "Chart request for token: {}, interval: {:?}, pagination: {}",
            token_id, chart_interval, pagination
        );
        let charts = sqlx::query_as::<_, Chart>(
            r#"
            SELECT 
                interval_type,
                token_id,                           
                open_price,
                close_price,
                high_price,
                low_price,
                volume,
                time_stamp
            FROM chart ch
            WHERE ch.token_id = $1
            AND ch.interval_type = $2
            ORDER BY ch.time_stamp DESC
            LIMIT 300
            OFFSET $3
            "#,
        )
        .bind(token_id)
        .bind(chart_interval)
        .bind(offset)
        .fetch_all(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow!("Failed to fetch chart: {}", err))?;

        Ok(charts)
    }
}
