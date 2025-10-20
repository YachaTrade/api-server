use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TimeFrame {
    #[serde(rename = "1")]
    OneMinute,
    #[serde(rename = "5")]
    FiveMinutes,
    #[serde(rename = "15")]
    FifteenMinutes,
    #[serde(rename = "30")]
    ThirtyMinutes,
    #[serde(rename = "60")]
    OneHour,
    #[serde(rename = "4H")]
    FourHours,
    #[serde(rename = "D")]
    OneDay,
    #[serde(rename = "W")]
    OneWeek,
    #[serde(rename = "M")]
    OneMonth,
}

impl TimeFrame {
    pub fn to_seconds(&self) -> i64 {
        match self {
            TimeFrame::OneMinute => 60,
            TimeFrame::FiveMinutes => 300,
            TimeFrame::FifteenMinutes => 900,
            TimeFrame::ThirtyMinutes => 1800,
            TimeFrame::OneHour => 3600,
            TimeFrame::FourHours => 14400,
            TimeFrame::OneDay => 86400,
            TimeFrame::OneWeek => 604800,
            TimeFrame::OneMonth => 2592000,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            TimeFrame::OneMinute => "1m",
            TimeFrame::FiveMinutes => "5m",
            TimeFrame::FifteenMinutes => "15m",
            TimeFrame::ThirtyMinutes => "30m",
            TimeFrame::OneHour => "1h",
            TimeFrame::FourHours => "4h",
            TimeFrame::OneDay => "1d",
            TimeFrame::OneWeek => "1w",
            TimeFrame::OneMonth => "1M",
        }
    }

    pub fn to_chart_interval(&self) -> &'static str {
        match self {
            TimeFrame::OneMinute => "1",
            TimeFrame::FiveMinutes => "5",
            TimeFrame::FifteenMinutes => "15",
            TimeFrame::ThirtyMinutes => "30",
            TimeFrame::OneHour => "1H",
            TimeFrame::FourHours => "4H",
            TimeFrame::OneDay => "1H",  // Use 1H interval for 24h metrics
            TimeFrame::OneWeek => "4H", // Use 4H interval for 1W metrics
            TimeFrame::OneMonth => "D", // Use D interval for 1M metrics
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionCount {
    pub buy: i64,
    pub sell: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VolumeAmount {
    pub buy: String,
    pub sell: String,
    pub total: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MakerCount {
    pub buy: i64,
    pub sell: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricItem {
    pub timeframe: String,
    pub percent: f64,
    pub transactions: TransactionCount,
    pub volume: VolumeAmount,
    pub makers: MakerCount,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsBatchResponse {
    pub metrics: Vec<MetricItem>,
}
