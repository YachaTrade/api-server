pub mod identifier;
pub mod info;
pub mod pagination;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CountRow {
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExistsRow {
    pub exists: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TotalValueRow {
    pub total_value: Option<sqlx::types::BigDecimal>,
}
