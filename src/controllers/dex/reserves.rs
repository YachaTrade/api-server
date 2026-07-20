use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase, measure_postgres, types::dex::reserves::ReservesResponse,
};

#[derive(Debug, sqlx::FromRow)]
struct ReservesRow {
    pool_id: String,
    token0: String,
    token1: String,
    reserve0: BigDecimal,
    reserve1: BigDecimal,
    block_number: i64,
}

pub struct ReservesController {
    db: Arc<PostgresDatabase>,
}

impl ReservesController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Fetch current reserves for a pool. `None` when the pool doesn't exist.
    pub async fn get_reserves(&self, pool_id: &str) -> Result<Option<ReservesResponse>> {
        let row = measure_postgres!(
            "dex.get_reserves",
            sqlx::query_as::<_, ReservesRow>(
                "SELECT pool_id, token0, token1, reserve0, reserve1, block_number \
                 FROM pool WHERE pool_id = $1",
            )
            .bind(pool_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to fetch reserves: {}", e))?;

        Ok(row.map(|r| ReservesResponse {
            pool_id: r.pool_id,
            token0: r.token0,
            token1: r.token1,
            reserve0: r.reserve0.normalized().to_plain_string(),
            reserve1: r.reserve1.normalized().to_plain_string(),
            block_number: r.block_number,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const POOL_ADDR: &str = "0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01";
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02";

    async fn seed_pool(pool: &PgPool) {
        sqlx::query(
            r#"INSERT INTO pool
                   (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                    latest_trade_at, created_at, block_number, tx_hash, total_supply)
               VALUES ($1, $2, $3, 60627715349858429639, 1263948, 2, 0, 1000,
                       0, 0, 987654, '0x', 5000)"#,
        )
        .bind(POOL_ADDR)
        .bind(TOKEN0)
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_controller(pool: PgPool) -> ReservesController {
        ReservesController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn returns_reserves_for_existing_pool(pool: PgPool) {
        seed_pool(&pool).await;
        let controller = make_controller(pool);
        let r = controller
            .get_reserves(POOL_ADDR)
            .await
            .unwrap()
            .expect("pool exists");
        assert_eq!(r.pool_id, POOL_ADDR);
        assert_eq!(r.token0, TOKEN0);
        assert_eq!(r.token1, TOKEN1);
        assert_eq!(r.reserve0, "60627715349858429639", "raw wei reserve0");
        assert_eq!(r.reserve1, "1263948", "raw wei reserve1");
        assert_eq!(r.block_number, 987654);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn returns_none_for_missing_pool(pool: PgPool) {
        let controller = make_controller(pool);
        let r = controller.get_reserves(POOL_ADDR).await.unwrap();
        assert!(r.is_none(), "missing pool -> None");
    }
}
