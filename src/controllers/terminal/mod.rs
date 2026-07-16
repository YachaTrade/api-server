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
    pub total_supply: BigDecimal,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PairRow {
    pub token_id: String,
    pub created_at: i64,
    pub transaction_hash: String,
    pub pool_id: Option<String>,
    pub market_type: String,
    pub quote_id: String,
    pub creator: String,
    pub creator_fee_rate: Option<i16>,
    pub curve_protocol_fee_rate: Option<i16>,
    pub dex_protocol_fee_rate: Option<i16>,
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

    /// Get asset (token) information by token_id
    pub async fn get_asset(&self, token_id: &str) -> Result<AssetRow> {
        let row = measure_postgres!(
            "terminal.get_asset",
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
            "terminal.get_pair",
            sqlx::query_as::<_, PairRow>(
                r#"
                    SELECT
                        t.token_id,
                        t.created_at,
                        t.transaction_hash,
                        m.pool_id,
                        m.market_type,
                        m.quote_id,
                        t.creator,
                        fc.creator_fee_rate,
                        fc.curve_protocol_fee_rate,
                        fc.dex_protocol_fee_rate
                    FROM token t
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN fee_config fc ON fc.token_id = t.token_id
                    WHERE t.token_id = $1
                      AND m.market_type IN ('DEX', 'V2_DEX')
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get pair: {}", err))?;

        Ok(row)
    }

    /// Get pair (token) information by pool_id
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
                        m.market_type,
                        m.quote_id,
                        t.creator,
                        fc.creator_fee_rate,
                        fc.curve_protocol_fee_rate,
                        fc.dex_protocol_fee_rate
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    LEFT JOIN fee_config fc ON fc.token_id = t.token_id
                    WHERE m.pool_id = $1
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
                        m.quote_id,
                        s.market_type
                    FROM swap s
                    JOIN market m ON s.token_id = m.token_id
                    WHERE s.block_number >= $1 AND s.block_number <= $2
                      AND s.market_type IN ('DEX', 'V2_DEX')
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
                      AND m.market_type IN ('DEX', 'V2_DEX')
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
                      AND m.market_type IN ('DEX', 'V2_DEX')
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
    pub market_type: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const TOKEN: &str = "0x00000000000000000000000000000000000000f1";
    const POOL: &str = "0x00000000000000000000000000000000000000C9";
    const QUOTE_LVMON: &str = "0xBe3fa50514D9617ce645a02B34F595541AF02b6b";

    fn ctrl(pool: PgPool) -> TerminalController {
        TerminalController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_token_market(pool: &PgPool, market_type: &str, pool_id: Option<&str>) {
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'me','','') ON CONFLICT DO NOTHING")
            .bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version)
            VALUES ($1,'T','T','',$2,NULL,false,false,false,100,'0xtx',1000000000000000000000000000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote)
            VALUES ($1,$2,$3,0,0,1,$4,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#)
            .bind(market_type).bind(TOKEN).bind(pool_id).bind(QUOTE_LVMON).execute(pool).await.unwrap();
    }

    async fn allow_future_market_type(pool: &PgPool) {
        sqlx::query(
            r#"
                DO $$
                DECLARE
                    constraint_name text;
                BEGIN
                    FOR constraint_name IN
                        SELECT c.conname
                        FROM pg_constraint c
                        WHERE c.conrelid = 'market'::regclass
                          AND c.contype = 'c'
                          AND pg_get_constraintdef(c.oid) LIKE '%market_type%'
                    LOOP
                        EXECUTE format(
                            'ALTER TABLE market DROP CONSTRAINT %I',
                            constraint_name
                        );
                    END LOOP;
                END
                $$
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_pair_returns_quote_id_and_fee_config(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO fee_config (pair_id,token_id,creator_fee_rate,curve_protocol_fee_rate,dex_protocol_fee_rate,created_at)
            VALUES ($1,$2,500,50,30,0) ON CONFLICT DO NOTHING"#)
            .bind(POOL).bind(TOKEN).execute(&pool).await.unwrap();

        let row = ctrl(pool).get_pair(TOKEN).await.unwrap();
        assert_eq!(row.quote_id, QUOTE_LVMON);
        assert_eq!(row.market_type, "V2_DEX");
        assert_eq!(row.creator_fee_rate, Some(500));
        assert_eq!(row.dex_protocol_fee_rate, Some(30));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_pair_without_fee_config_is_none(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        let row = ctrl(pool).get_pair(TOKEN).await.unwrap();
        assert_eq!(row.quote_id, QUOTE_LVMON);
        assert_eq!(row.creator_fee_rate, None);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_pair_only_returns_dex_markets(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;

        for market_type in ["DEX", "V2_DEX"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();

            let row = ctrl(pool.clone()).get_pair(TOKEN).await.unwrap();
            assert_eq!(row.market_type, market_type);
        }

        for market_type in ["CURVE", "V2_CURVE"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();

            assert!(
                ctrl(pool.clone()).get_pair(TOKEN).await.is_err(),
                "{market_type} pair must be filtered out"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_swap_events_carry_quote_id_and_market_type(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO swap (account_id,token_id,market_type,is_buy,quote_amount,token_amount,reserve_quote,reserve_token,value,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,'V2_DEX',true,1000000000000000000,2000000000000000000,100,200,0,123,'0xs',10,0,0) ON CONFLICT DO NOTHING"#)
            .bind(ACCOUNT).bind(TOKEN).execute(&pool).await.unwrap();

        let rows = ctrl(pool).get_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
        assert_eq!(rows[0].market_type, "V2_DEX");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_events_only_returns_dex_and_uses_swap_market_type(pool: PgPool) {
        // Current market stays V2_DEX; historical eligibility must use each swap row.
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;

        for (market_type, tx_hash, block_number) in [
            ("CURVE", "0xcurve", 10_i64),
            ("DEX", "0xdex", 11_i64),
            ("V2_CURVE", "0xv2curve", 12_i64),
            ("V2_DEX", "0xv2dex", 13_i64),
        ] {
            sqlx::query(
                r#"INSERT INTO swap
                    (account_id,token_id,market_type,is_buy,quote_amount,token_amount,
                     reserve_quote,reserve_token,value,created_at,transaction_hash,
                     block_number,tx_index,log_index)
                   VALUES ($1,$2,$3,true,1,2,100,200,0,$4,$5,$4,0,0)"#,
            )
            .bind(ACCOUNT)
            .bind(TOKEN)
            .bind(market_type)
            .bind(block_number)
            .bind(tx_hash)
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows = ctrl(pool).get_events(0, 100).await.unwrap();
        let markets: Vec<&str> = rows.iter().map(|row| row.market_type.as_str()).collect();
        let transactions: Vec<&str> = rows
            .iter()
            .map(|row| row.transaction_hash.as_str())
            .collect();

        assert_eq!(markets, vec!["DEX", "V2_DEX"]);
        assert_eq!(transactions, vec!["0xdex", "0xv2dex"]);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_mint_events_only_return_dex_markets(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO mint (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,$3,10,20,100,200,123,'0xm',10,0,0)"#)
            .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

        allow_future_market_type(&pool).await;

        for market_type in ["DEX", "V2_DEX"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();
            let rows = ctrl(pool.clone()).get_mint_events(0, 100).await.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].quote_id, QUOTE_LVMON);
        }

        for market_type in ["FUTURE_MARKET", "CURVE", "V2_CURVE"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();
            let rows = ctrl(pool.clone()).get_mint_events(0, 100).await.unwrap();
            assert!(rows.is_empty(), "{market_type} mint must be filtered out");
        }
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_burn_events_only_return_dex_markets(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO burn (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,$3,10,20,100,200,123,'0xb',10,0,0)"#)
            .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

        allow_future_market_type(&pool).await;

        for market_type in ["DEX", "V2_DEX"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();
            let rows = ctrl(pool.clone()).get_burn_events(0, 100).await.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].quote_id, QUOTE_LVMON);
        }

        for market_type in ["FUTURE_MARKET", "CURVE", "V2_CURVE"] {
            sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
                .bind(market_type)
                .bind(TOKEN)
                .execute(&pool)
                .await
                .unwrap();
            let rows = ctrl(pool.clone()).get_burn_events(0, 100).await.unwrap();
            assert!(rows.is_empty(), "{market_type} burn must be filtered out");
        }
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn block_number_by_tx_none_when_no_balance_history(pool: PgPool) {
        // A token created without an initial buy has no balance_history row for
        // its creation tx. get_block_number_by_tx must return None, not error.
        let got = ctrl(pool)
            .get_block_number_by_tx("0xnotindexed")
            .await
            .unwrap();
        assert_eq!(got, None);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn block_number_by_tx_some_when_present(pool: PgPool) {
        sqlx::query(r#"INSERT INTO balance_history (token_id,account_id,balance,block_number,transaction_hash,log_index,tx_index)
            VALUES ($1,$2,0,12345,'0xseedtx',0,0)"#)
            .bind(TOKEN).bind(ACCOUNT).execute(&pool).await.unwrap();
        let got = ctrl(pool).get_block_number_by_tx("0xseedtx").await.unwrap();
        assert_eq!(got, Some(12345));
    }
}
