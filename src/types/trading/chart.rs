use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::pagination::{deserialize_chart_limit, DEFAULT_CHART_LIMIT};

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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Price,        // MON/TOKEN 가격 (기본)
    PriceUsd,     // USD 가격
    MarketCap,    // 시가총액 (MON) = price * total_supply
    MarketCapUsd, // 시가총액 (USD) = usd_price * total_supply
}

impl Default for ChartType {
    fn default() -> Self {
        ChartType::Price
    }
}

fn default_countback() -> i32 {
    DEFAULT_CHART_LIMIT
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct GetBarsRequest {
    #[serde(default = "default_resolution")]
    pub resolution: String, // 타임프레임 (1, 5, 15, 30, 60, 1H, 4H, D, W, M)
    pub from: i64, // 시작 타임스탬프 (초 단위)
    pub to: i64,   // 끝 타임스탬프 (초 단위)
    #[serde(default = "default_countback", deserialize_with = "deserialize_chart_limit")]
    pub countback: i32, // 반환할 최대 캔들 수 (최대 1000)
    #[serde(default)]
    pub chart_type: ChartType, // 차트 타입 (price, price_usd, market_cap, market_cap_usd)
}

fn default_resolution() -> String {
    "5".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BarResponse {
    pub k: String,      //chart type
    pub t: Vec<i64>,    // 타임스탬프 배열 (초 단위)
    pub c: Vec<String>, // 종가 배열 (문자열로 반환)
    pub o: Vec<String>, // 시가 배열 (문자열로 반환)
    pub h: Vec<String>, // 고가 배열 (문자열로 반환)
    pub l: Vec<String>, // 저가 배열 (문자열로 반환)
    pub v: Vec<String>, // 거래량 배열 (문자열로 반환)
    pub s: String,      // 상태 코드 ("ok" 또는 "error" 또는 "no_data")
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
