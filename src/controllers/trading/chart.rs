use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::trading::chart::{BarResponse, Chart, GetBarsRequest},
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct ChartController {
    pub db: Arc<PostgresDatabase>,
}

impl ChartController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ChartController { db }
    }

    pub async fn get_prices(
        &self,
        token_id: &str,
        request: &GetBarsRequest,
    ) -> Result<BarResponse> {
        let cache_key = cache_key!(
            "chart",
            token_id,
            &request.resolution,
            request.from,
            request.to,
            request.countback
        );

        let result = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_chart_data(token_id, request).await
        })
        .await?;

        Ok(result)
    }

    async fn fetch_chart_data(
        &self,
        token_id: &str,
        request: &GetBarsRequest,
    ) -> Result<BarResponse> {
        let interval_type = resolution_to_interval_type(&request.resolution)?;

        let limit = match request.countback {
            Some(0) | None => 500,
            Some(count) => count,
        };

        let query = r#"
            SELECT 
                interval_type,
                token_id,
                open_price,
                close_price,
                high_price,
                low_price,
                volume,
                time_stamp
            FROM chart
            WHERE token_id = $1
            AND interval_type = $2
            AND time_stamp >= $3
            AND time_stamp <= $4
            ORDER BY time_stamp DESC
            LIMIT $5
        "#;

        let mut charts = measure_postgres!(
            "chart.fetch_chart_data",
            sqlx::query_as::<_, Chart>(&query)
                .bind(token_id)
                .bind(interval_type)
                .bind(request.from)
                .bind(request.to)
                .bind(limit as i32)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch chart: {}", err))?;

        charts.reverse();

        if charts.is_empty() {
            return Ok(BarResponse {
                t: vec![],
                c: vec![],
                o: vec![],
                h: vec![],
                l: vec![],
                v: vec![],
                s: "no_data".to_string(),
            });
        }

        let mut t = Vec::with_capacity(charts.len());
        let mut c = Vec::with_capacity(charts.len());
        let mut o = Vec::with_capacity(charts.len());
        let mut h = Vec::with_capacity(charts.len());
        let mut l = Vec::with_capacity(charts.len());
        let mut v = Vec::with_capacity(charts.len());

        for chart in charts {
            t.push(chart.time_stamp);
            c.push(chart.close_price.to_plain_string());
            o.push(chart.open_price.to_plain_string());
            h.push(chart.high_price.to_plain_string());
            l.push(chart.low_price.to_plain_string());
            v.push(chart.volume.to_plain_string());
        }

        Ok(BarResponse {
            t,
            c,
            o,
            h,
            l,
            v,
            s: "ok".to_string(),
        })
    }
}

fn resolution_to_interval_type(resolution: &str) -> Result<&'static str> {
    match resolution {
        "1" => Ok("1"),
        "5" => Ok("5"),
        "15" => Ok("15"),
        "30" => Ok("30"),
        "60" | "1H" => Ok("1H"),
        "4H" => Ok("4H"),
        "D" | "1D" => Ok("D"),
        "W" | "1W" => Ok("W"),
        "M" | "1M" => Ok("M"),
        _ => Err(anyhow!("Invalid resolution: {}", resolution)),
    }
}
