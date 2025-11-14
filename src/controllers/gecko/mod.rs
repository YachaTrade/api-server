use std::sync::Arc;

use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;

use crate::{db::postgres::PostgresDatabase, measure_postgres};

#[derive(Debug, sqlx::FromRow)]
struct LatestBlockRow {
    block_number: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AssetRow {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub total_supply: BigDecimal,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PairRow {
    pub token_id: String,
    pub created_at: i64,
    pub transaction_hash: String,
    pub pool_id: Option<String>,
    pub market_type: String,
    pub creator: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BlockNumberRow {
    pub block_number: i64,
}

pub struct GeckoController {
    db: Arc<PostgresDatabase>,
}

impl GeckoController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        GeckoController { db }
    }

    /// Get the latest indexed block number from balance_history table
    pub async fn get_latest_indexed_block(&self) -> Result<u64> {
        let row = measure_postgres!(
            "gecko.get_latest_indexed_block",
            sqlx::query_as::<_, LatestBlockRow>(
                r#"
                    SELECT MAX(block_number) as block_number
                    FROM balance_history
                "#,
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get latest indexed block: {}", err))?;

        Ok(row.block_number as u64)
    }

    /// Get asset (token) information by token_id
    pub async fn get_asset(&self, token_id: &str) -> Result<AssetRow> {
        let row = measure_postgres!(
            "gecko.get_asset",
            sqlx::query_as::<_, AssetRow>(
                r#"
                    SELECT
                        token_id,
                        name,
                        symbol,
                        total_supply
                    FROM token
                    WHERE token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get asset: {}", err))?;

        Ok(row)
    }

    /// Get pair (token) information by token_id
    pub async fn get_pair(&self, token_id: &str) -> Result<PairRow> {
        let row = measure_postgres!(
            "gecko.get_pair",
            sqlx::query_as::<_, PairRow>(
                r#"
                    SELECT
                        t.token_id,
                        t.created_at,
                        t.transaction_hash,
                        m.pool_id,
                        m.market_type,
                        t.creator
                    FROM token t
                    JOIN market m ON t.token_id = m.token_id
                    WHERE t.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get pair: {}", err))?;

        Ok(row)
    }

    /// Get block number by transaction hash from balance_history
    pub async fn get_block_number_by_tx(&self, transaction_hash: &str) -> Result<u64> {
        let row = measure_postgres!(
            "gecko.get_block_number_by_tx",
            sqlx::query_as::<_, BlockNumberRow>(
                r#"
                    SELECT block_number
                    FROM balance_history
                    WHERE transaction_hash = $1
                    LIMIT 1
                "#,
            )
            .bind(transaction_hash)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get block number by transaction hash: {}", err))?;

        Ok(row.block_number as u64)
    }

    /// Get swap events from swap table for a block range
    pub async fn get_events(&self, from_block: u64, to_block: u64) -> Result<Vec<SwapEventRow>> {
        let rows = measure_postgres!(
            "gecko.get_events",
            sqlx::query_as::<_, SwapEventRow>(
                r#"
                    SELECT
                        s.account_id,
                        s.token_id,
                        s.is_buy,
                        s.native_amount,
                        s.token_amount,
                        s.reserve_native,
                        s.reserve_token,
                        s.created_at,
                        s.transaction_hash,
                        s.block_number,
                        s.tx_index,
                        s.log_index,
                        m.pool_id,
                        m.price
                    FROM swap s
                    JOIN market m ON s.token_id = m.token_id
                    WHERE s.block_number >= $1 AND s.block_number <= $2
                    ORDER BY s.block_number ASC, s.tx_index ASC, s.log_index ASC
                "#,
            )
            .bind(from_block as i64)
            .bind(to_block as i64)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get swap events: {}", err))?;

        Ok(rows)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct SwapEventRow {
    pub account_id: String,
    pub token_id: String,
    pub is_buy: bool,
    pub native_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub reserve_native: Option<BigDecimal>,
    pub reserve_token: Option<BigDecimal>,
    pub created_at: i64,
    pub transaction_hash: String,
    pub block_number: i64,
    pub tx_index: Option<i32>,
    pub log_index: i32,
    pub pool_id: Option<String>,
    pub price: BigDecimal,
}
