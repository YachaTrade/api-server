use std::sync::Arc;

use anyhow::{Result, anyhow};
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
    pub decimals: i32,
    /// nadfun `token` 테이블에서 온 자산만 Some — quote 자산은
    /// 공급량을 추적하지 않으므로 None (응답에서 필드 생략).
    pub total_supply: Option<BigDecimal>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PairRow {
    pub token_id: String,
    pub created_at: i64,
    pub transaction_hash: String,
    pub pool_id: Option<String>,
    pub quote_id: String,
    pub creator: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BlockNumberRow {
    pub block_number: i64,
}

pub struct TerminalController {
    db: Arc<PostgresDatabase>,
}

impl TerminalController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TerminalController { db }
    }

    /// Get the latest indexed block number from balance_history table
    pub async fn get_latest_indexed_block(&self) -> Result<u64> {
        let row = measure_postgres!(
            "terminal.get_latest_indexed_block",
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

    /// Get asset information by address, resolving across every table an
    /// asset id we emit can live in: nadfun `token` first (the only source
    /// with supply data), then `quote_token` (the quote side of every /pair).
    pub async fn get_asset(&self, token_id: &str) -> Result<AssetRow> {
        let row = measure_postgres!(
            "terminal.get_asset",
            sqlx::query_as::<_, AssetRow>(
                r#"
                    SELECT token_id, name, symbol, 18 AS decimals, total_supply, 1 AS priority
                    FROM token WHERE token_id = $1
                    UNION ALL
                    SELECT quote_id, name, symbol, decimals, NULL::numeric, 2
                    FROM quote_token WHERE quote_id = $1
                    ORDER BY priority
                    LIMIT 1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get asset: {}", err))?;

        Ok(row)
    }

    /// Get pair (token) information by token_id
    pub async fn get_pair_by_pool_id(&self, pool_id: &str) -> Result<PairRow> {
        let row = measure_postgres!(
            "terminal.get_pair_by_pool_id",
            sqlx::query_as::<_, PairRow>(
                r#"
                    SELECT
                        t.token_id,
                        t.created_at,
                        t.transaction_hash,
                        m.pool_id,
                        m.quote_id,
                        t.creator
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    WHERE m.pool_id = $1
                      AND m.market_type = 'DEX'
                "#,
            )
            .bind(pool_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get pair by pool_id: {}", err))?;

        Ok(row)
    }

    /// Get block number by transaction hash from balance_history.
    ///
    /// Returns `None` when the tx has no balance_history row — e.g. a token
    /// created without an initial buy produces no balance change, so its
    /// creation tx is absent here. Callers treat the block number as optional
    /// rather than failing the request.
    pub async fn get_block_number_by_tx(&self, transaction_hash: &str) -> Result<Option<u64>> {
        let row = measure_postgres!(
            "terminal.get_block_number_by_tx",
            sqlx::query_as::<_, BlockNumberRow>(
                r#"
                    SELECT block_number
                    FROM balance_history
                    WHERE transaction_hash = $1
                    LIMIT 1
                "#,
            )
            .bind(transaction_hash)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get block number by transaction hash: {}", err))?;

        Ok(row.map(|r| r.block_number as u64))
    }

    /// Get swap events from swap table for a block range
    pub async fn get_events(&self, from_block: u64, to_block: u64) -> Result<Vec<SwapEventRow>> {
        let rows = measure_postgres!(
            "terminal.get_events",
            sqlx::query_as::<_, SwapEventRow>(
                r#"
                    SELECT
                        s.account_id,
                        s.token_id,
                        s.is_buy,
                        s.quote_amount,
                        s.token_amount,
                        s.reserve_quote,
                        s.reserve_token,
                        s.created_at,
                        s.transaction_hash,
                        s.block_number,
                        s.tx_index,
                        s.log_index,
                        m.pool_id,
                        m.price,
                        m.quote_id
                    FROM swap s
                    JOIN market m ON s.token_id = m.token_id
                    WHERE s.block_number >= $1 AND s.block_number <= $2
                      AND s.market_type = 'DEX'
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

    /// Get mint events from mint table for a block range
    pub async fn get_mint_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<MintEventRow>> {
        let rows = measure_postgres!(
            "terminal.get_mint_events",
            sqlx::query_as::<_, MintEventRow>(
                r#"
                    SELECT
                        mn.token_id,
                        mn.account_id,
                        mn.market_id,
                        mn.quote_amount,
                        mn.token_amount,
                        mn.reserve_quote,
                        mn.reserve_token,
                        mn.created_at,
                        mn.transaction_hash,
                        mn.block_number,
                        mn.tx_index,
                        mn.log_index,
                        m.quote_id
                    FROM mint mn
                    JOIN market m ON m.token_id = mn.token_id
                    WHERE mn.block_number >= $1 AND mn.block_number <= $2
                      AND m.market_type = 'DEX'
                    ORDER BY mn.block_number ASC, mn.tx_index ASC, mn.log_index ASC
                "#,
            )
            .bind(from_block as i64)
            .bind(to_block as i64)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get mint events: {}", err))?;

        Ok(rows)
    }

    /// Get burn events from burn table for a block range
    pub async fn get_burn_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<BurnEventRow>> {
        let rows = measure_postgres!(
            "terminal.get_burn_events",
            sqlx::query_as::<_, BurnEventRow>(
                r#"
                    SELECT
                        bn.token_id,
                        bn.account_id,
                        bn.market_id,
                        bn.quote_amount,
                        bn.token_amount,
                        bn.reserve_quote,
                        bn.reserve_token,
                        bn.created_at,
                        bn.transaction_hash,
                        bn.block_number,
                        bn.tx_index,
                        bn.log_index,
                        m.quote_id
                    FROM burn bn
                    JOIN market m ON m.token_id = bn.token_id
                    WHERE bn.block_number >= $1 AND bn.block_number <= $2
                      AND m.market_type = 'DEX'
                    ORDER BY bn.block_number ASC, bn.tx_index ASC, bn.log_index ASC
                "#,
            )
            .bind(from_block as i64)
            .bind(to_block as i64)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get burn events: {}", err))?;

        Ok(rows)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct SwapEventRow {
    pub account_id: String,
    pub token_id: String,
    pub is_buy: bool,
    pub quote_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub reserve_quote: Option<BigDecimal>,
    pub reserve_token: Option<BigDecimal>,
    pub created_at: i64,
    pub transaction_hash: String,
    pub block_number: i64,
    pub tx_index: Option<i32>,
    pub log_index: i32,
    pub pool_id: Option<String>,
    pub price: BigDecimal,
    pub quote_id: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MintEventRow {
    pub token_id: String,
    pub account_id: String,
    pub market_id: String,
    pub quote_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub reserve_quote: BigDecimal,
    pub reserve_token: BigDecimal,
    pub created_at: i64,
    pub transaction_hash: String,
    pub block_number: i64,
    pub tx_index: i32,
    pub log_index: i32,
    pub quote_id: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct BurnEventRow {
    pub token_id: String,
    pub account_id: String,
    pub market_id: String,
    pub quote_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub reserve_quote: BigDecimal,
    pub reserve_token: BigDecimal,
    pub created_at: i64,
    pub transaction_hash: String,
    pub block_number: i64,
    pub tx_index: i32,
    pub log_index: i32,
    pub quote_id: String,
}
