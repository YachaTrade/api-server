use crate::env;
use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Account {
    // #[serde(rename = "id")]
    pub account_id: String,

    // #[serde(rename = "imageUri")]
    pub image_uri: String,

    // #[serde(rename = "nickname")]
    pub nickname: String,

    // #[serde(rename = "bio")]
    pub bio: String,

    // #[serde(rename = "followerCount")]
    pub follower_count: i32,

    // #[serde(rename = "followingCount")]
    pub following_count: i32,

    // #[serde(rename = "likeCount")]
    pub like_count: i32,
}

impl Account {
    pub fn new(account_id: String) -> Self {
        //random_number 는 1~5까지의 숫자가 나와야함.
        let random_number = rand::thread_rng().gen_range(1..=5);
        let image_key = format!("DEFAULT_IMAGE_{}", random_number);
        let image_uri = env::get_env(&image_key);
        Self {
            account_id: account_id.clone(),
            image_uri,
            nickname: account_id,
            bio: "".to_string(),
            follower_count: 0,
            following_count: 0,
            like_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Token {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub creator: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub image_uri: String,
    pub is_listing: bool,
    pub pair: Option<String>,
    pub created_at: i64,
    pub create_transaction_hash: String,
    #[serde(skip_serializing)]
    pub is_updated: bool,
}
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
    pub volume: i64,
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

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Thread {
    pub thread_id: i32,
    pub token_id: String,
    pub account_id: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub root_id: Option<i32>,
    pub likes_count: i32,
    pub reply_count: i32,
    pub image_uri: Option<String>,
}
impl Thread {
    pub fn new(
        token_id: String,
        account_id: String,
        content: String,
        root_id: Option<i32>,
    ) -> Self {
        Self {
            thread_id: 0,
            token_id,
            account_id,
            content,
            created_at: Utc::now(),
            root_id,
            likes_count: 0,
            reply_count: 0,
            image_uri: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct MintParty {
    pub mint_party_id: String,
    pub account_id: String,
    pub funding_amount: BigDecimal,
    pub allow_white_list_count: i16,
    pub current_white_list_count: i16,
    pub total_deposit_amount: BigDecimal,
    pub is_finished: bool,
    pub is_closed: bool,
    pub created_at: i64,
    pub token_id: Option<String>,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub transaction_hash: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    #[serde(skip_serializing)]
    pub is_updated: bool,
}
