use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Chart {
    #[serde(skip_serializing)]
    pub interval_type: i16,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartInterval {
    Minute1 = 1,
    Minute5 = 2,
    Minute15 = 3,
    Minute30 = 4,
    Hour1 = 5,
    Hour4 = 6,
    Day1 = 7,
    Week1 = 8,
}

impl ChartInterval {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "1m" => Ok(Self::Minute1),
            "5m" => Ok(Self::Minute5),
            "15m" => Ok(Self::Minute15),
            "30m" => Ok(Self::Minute30),
            "1h" => Ok(Self::Hour1),
            "4h" => Ok(Self::Hour4),
            "1d" => Ok(Self::Day1),
            "1w" => Ok(Self::Week1),
            _ => Err(anyhow!("Invalid interval: {}", s)),
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Minute1 => "1m",
            Self::Minute5 => "5m",
            Self::Minute15 => "15m",
            Self::Minute30 => "30m",
            Self::Hour1 => "1h",
            Self::Hour4 => "4h",
            Self::Day1 => "1d",
            Self::Week1 => "1w",
        }
    }

    pub fn from_i16(value: i16) -> Result<Self> {
        match value {
            1 => Ok(Self::Minute1),
            2 => Ok(Self::Minute5),
            3 => Ok(Self::Minute15),
            4 => Ok(Self::Minute30),
            5 => Ok(Self::Hour1),
            6 => Ok(Self::Hour4),
            7 => Ok(Self::Day1),
            8 => Ok(Self::Week1),
            _ => Err(anyhow!("Invalid interval value: {}", value)),
        }
    }

    // i16 -> str 직접 변환 메서드
    pub fn i16_to_string(value: i16) -> Result<String> {
        Self::from_i16(value).map(|interval| interval.to_str().to_string())
    }
}

impl From<ChartInterval> for i16 {
    fn from(interval: ChartInterval) -> i16 {
        interval as i16
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
        // resolution을 ChartInterval로 변환
        let interval = match request.resolution.as_str() {
            "1" => ChartInterval::Minute1,
            "5" => ChartInterval::Minute5,
            "15" => ChartInterval::Minute15,
            "30" => ChartInterval::Minute30,
            "60" | "1H" => ChartInterval::Hour1,
            "4H" => ChartInterval::Hour4,
            "D" => ChartInterval::Day1,
            "W" => ChartInterval::Week1,
            _ => return Err(anyhow!("Invalid resolution: {}", request.resolution)),
        };

        let chart_interval: i16 = interval.into();

        // countback이 제공되었다면 사용, 아니면 기본값 500
        let limit = request.countback.unwrap_or(500);

        let query = format!(
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
            FROM chart
            WHERE token_id = $1
            AND interval_type = $2
            AND time_stamp >= $3
            AND time_stamp <= $4
            ORDER BY time_stamp ASC
            LIMIT $5
            "#
        );

        let charts = sqlx::query_as::<_, Chart>(&query)
            .bind(token_id)
            .bind(chart_interval)
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
