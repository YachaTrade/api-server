use std::sync::Arc;

use crate::db::postgres::{
    model::{Account, Chart, ChartInterval},
    PostgresDatabase,
};

use anyhow::{anyhow, Context, Result};

pub struct ChartController {
    pub db: Arc<PostgresDatabase>,
}

impl<'a> ChartController {
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
        let offset = (pagination as i64) * 300;

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
        .await?;

        Ok(charts)
    }
}
