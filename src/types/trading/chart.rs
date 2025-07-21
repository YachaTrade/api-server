use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::info;
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Chart {
    #[serde(skip_serializing)]
    pub interval_type: String,
    #[serde(skip_serializing)]
    pub token_id: String,
    pub open_price: BigDecimal,
    pub close_price: BigDecimal,
    pub high_price: BigDecimal,
    pub low_price: BigDecimal,
    pub volume: BigDecimal,
    pub time_stamp: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct GetBarsRequest {
    #[serde(default = "default_resolution")]
    pub resolution: String, // 타임프레임 (1, 5, 15, 30, 60, D, W, M)
    pub from: i64, // 시작 타임스탬프 (초 단위)
    pub to: i64,   // 끝 타임스탬프 (초 단위)
    #[serde(default = "default_countback")]
    pub countback: Option<i32>, // 반환할 최대 캔들 수
}
fn default_resolution() -> String {
    "5".to_string()
}

fn default_countback() -> Option<i32> {
    Some(500)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BarResponse {
    pub t: Vec<i64>,    // 타임스탬프 배열 (초 단위)
    pub c: Vec<String>, // 종가 배열 (문자열로 반환)
    pub o: Vec<String>, // 시가 배열 (문자열로 반환)
    pub h: Vec<String>, // 고가 배열 (문자열로 반환)
    pub l: Vec<String>, // 저가 배열 (문자열로 반환)
    pub v: Vec<String>, // 거래량 배열 (문자열로 반환)
    pub s: String,      // 상태 코드 ("ok" 또는 "error" 또는 "no_data")
}

// Helper function to convert resolution to DB interval_type
fn resolution_to_interval_type(resolution: &str) -> Result<&'static str> {
    match resolution {
        "1" => Ok("1"),
        "5" => Ok("5"),
        "15" => Ok("15"),
        "30" => Ok("30"),
        "60" | "1H" => Ok("60"),
        "240" | "4H" => Ok("D"), // Using 'D' for 4H as closest match
        "D" | "1D" => Ok("D"),
        "W" | "1W" => Ok("W"),
        "M" | "1M" => Ok("M"),
        _ => Err(anyhow!("Invalid resolution: {}", resolution)),
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChartResponse {
    pub data: Vec<Chart>,
    pub token_id: String,
    pub interval: String,
    pub base_timestamp: i64,
    // pub total_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChartQuery {
    pub interval: String,
    pub base_timestamp: i64,
}

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
        let start_time = Instant::now();
        // resolution을 DB interval_type으로 변환
        let interval_type = resolution_to_interval_type(&request.resolution)?;

        // countback이 제공되었다면 사용, 아니면 기본값 500
        // countback이 0이면 기본값 사용
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
            ORDER BY time_stamp ASC
            LIMIT $5
        "#;

        let charts = sqlx::query_as::<_, Chart>(&query)
            .bind(token_id)
            .bind(interval_type)
            .bind(request.from)
            .bind(request.to)
            .bind(limit as i32)
            .fetch_all(self.db.get_read_pool())
            .await
            .map_err(|err| anyhow!("Failed to fetch chart: {}", err))?;

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

        // 차트 데이터를 BarData 형식으로 변환
        let mut t = Vec::with_capacity(charts.len());
        let mut c = Vec::with_capacity(charts.len());
        let mut o = Vec::with_capacity(charts.len());
        let mut h = Vec::with_capacity(charts.len());
        let mut l = Vec::with_capacity(charts.len());
        let mut v = Vec::with_capacity(charts.len());

        for chart in charts {
            t.push(chart.time_stamp);
            c.push(chart.close_price.to_string());
            o.push(chart.open_price.to_string());
            h.push(chart.high_price.to_string());
            l.push(chart.low_price.to_string());
            v.push(chart.volume.to_string());
        }

        let elapsed = start_time.elapsed();
        info!(
            "get_prices completed in {:?} for token_id: {}, resolution: {}, from: {}, to: {}",
            elapsed, token_id, request.resolution, request.from, request.to
        );
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
