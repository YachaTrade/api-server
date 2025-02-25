use std::sync::Arc;

use anyhow::{anyhow, Result};
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
    pub pagination: i64,
    pub total_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChartQuery {
    pub interval: String,
    pub pagination: Option<i64>,
}

pub struct ChartController {
    pub db: Arc<PostgresDatabase>,
}

impl ChartController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ChartController { db }
    }
    pub async fn get_total_count(&self, token_id: &str, interval: ChartInterval) -> Result<i64> {
        let chart_interval: i16 = interval.into();
        let count = sqlx::query!(
            r#"
            SELECT 
                COUNT(*)
            FROM chart
            WHERE token_id = $1
                AND interval_type = $2
            "#,
            token_id,
            chart_interval
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }

    pub async fn get_chart(
        &self,
        token_id: &str,
        interval: ChartInterval,
        pagination: i64,
    ) -> Result<ChartResponse> {
        let chart_interval: i16 = interval.into();
        let pagination = if pagination <= 0 { 1 } else { pagination };
        let offset = (pagination - 1) * 300;
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

        let total_count = if charts.is_empty() {
            0
        } else {
            self.get_total_count(token_id, interval).await?
        };

        let chart_data = charts.into_iter().map(Chart::from).collect();

        Ok(ChartResponse {
            data: chart_data,
            token_id: token_id.to_string(),
            interval: ChartInterval::i16_to_string(chart_interval)?,
            pagination,
            total_count,
        })
    }
}
