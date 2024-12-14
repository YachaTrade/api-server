use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MintPartyOrderType {
    CreationTime,
    FundingAmount,
}
impl MintPartyOrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MintPartyOrderType::CreationTime => "creation_time",
            MintPartyOrderType::FundingAmount => "funding_amount",
        }
    }
}
impl Default for MintPartyOrderType {
    fn default() -> Self {
        Self::CreationTime // 기본값으로 생성시간 정렬 사용
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl Default for OrderDirection {
    fn default() -> Self {
        Self::Desc // 기본값은 내림차순
    }
}

impl OrderDirection {
    pub fn as_sql(&self) -> &'static str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}
