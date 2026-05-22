use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    controllers::dex::position::apr_max_pct,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::pool::{FeeConfigInfo, PoolDetailResponse, PoolTokenSide},
    types::dex::pool_info::PoolInfo,
};

#[derive(Debug, sqlx::FromRow)]
struct PoolRow {
    pool_id: String,
    token0: String,
    token1: String,
    reserve0: BigDecimal,
    reserve1: BigDecimal,
    pool_value_usd: BigDecimal,
    pool_total_supply: BigDecimal,
    token0_symbol: Option<String>,
    token0_decimals: Option<i32>,
    token0_image: Option<String>,
    token1_symbol: Option<String>,
    token1_decimals: Option<i32>,
    token1_image: Option<String>,
    lp_fee_24h_usd: Option<f64>,
    tvl_24h_usd_avg: Option<f64>,
    lp_fee_7d_usd: Option<f64>,
    tvl_7d_usd_avg: Option<f64>,
    lp_fee_30d_usd: Option<f64>,
    tvl_30d_usd_avg: Option<f64>,
    fee_creator_bps: Option<i16>,
    fee_curve_bps: Option<i16>,
    fee_dex_bps: Option<i16>,
}

pub struct PoolController {
    db: Arc<PostgresDatabase>,
}

impl PoolController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_pool_detail(&self, pool_id: &str) -> Result<Option<PoolDetailResponse>> {
        let row = measure_postgres!(
            "dex.get_pool_detail",
            sqlx::query_as::<_, PoolRow>(
                r#"
                SELECT
                    p.pool_id,
                    p.token0,
                    p.token1,
                    p.reserve0,
                    p.reserve1,
                    p.value        AS pool_value_usd,
                    p.total_supply AS pool_total_supply,
                    COALESCE(t0.symbol,    dt0.symbol,    qt0.symbol)    AS token0_symbol,
                    COALESCE(dt0.decimals, qt0.decimals)                 AS token0_decimals,
                    COALESCE(t0.image_uri, dt0.image_uri, qt0.image_uri) AS token0_image,
                    COALESCE(t1.symbol,    dt1.symbol,    qt1.symbol)    AS token1_symbol,
                    COALESCE(dt1.decimals, qt1.decimals)                 AS token1_decimals,
                    COALESCE(t1.image_uri, dt1.image_uri, qt1.image_uri) AS token1_image,
                    par.lp_fee_24h_usd::float8  AS lp_fee_24h_usd,
                    par.tvl_24h_usd_avg::float8 AS tvl_24h_usd_avg,
                    par.lp_fee_7d_usd::float8   AS lp_fee_7d_usd,
                    par.tvl_7d_usd_avg::float8  AS tvl_7d_usd_avg,
                    par.lp_fee_30d_usd::float8  AS lp_fee_30d_usd,
                    par.tvl_30d_usd_avg::float8 AS tvl_30d_usd_avg,
                    fc.creator_fee_rate        AS fee_creator_bps,
                    fc.curve_protocol_fee_rate AS fee_curve_bps,
                    fc.dex_protocol_fee_rate   AS fee_dex_bps
                FROM pool p
                LEFT JOIN token       t0  ON t0.token_id  = p.token0
                LEFT JOIN dex_token   dt0 ON dt0.token_id = p.token0
                LEFT JOIN quote_token qt0 ON qt0.quote_id = p.token0
                LEFT JOIN token       t1  ON t1.token_id  = p.token1
                LEFT JOIN dex_token   dt1 ON dt1.token_id = p.token1
                LEFT JOIN quote_token qt1 ON qt1.quote_id = p.token1
                LEFT JOIN pool_apr    par ON par.pool_id  = p.pool_id
                LEFT JOIN fee_config  fc  ON fc.pair_id   = p.pool_id
                WHERE p.pool_id = $1
                "#,
            )
            .bind(pool_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to fetch pool detail: {}", e))?;

        Ok(row.map(row_to_response))
    }
}

fn row_to_response(r: PoolRow) -> PoolDetailResponse {
    let token0_symbol = r.token0_symbol.unwrap_or_default();
    let token1_symbol = r.token1_symbol.unwrap_or_default();
    let pair_label = format!("{}-{}", token0_symbol, token1_symbol);

    let apr = apr_max_pct(
        r.lp_fee_24h_usd, r.tvl_24h_usd_avg,
        r.lp_fee_7d_usd,  r.tvl_7d_usd_avg,
        r.lp_fee_30d_usd, r.tvl_30d_usd_avg,
    );

    let fee_config = match (r.fee_creator_bps, r.fee_curve_bps, r.fee_dex_bps) {
        (Some(c), Some(p), Some(d)) => Some(FeeConfigInfo {
            creator_bps: c,
            curve_protocol_bps: p,
            dex_protocol_bps: d,
        }),
        _ => None,
    };

    PoolDetailResponse {
        pool_info: PoolInfo {
            pool_id: r.pool_id,
            pair_label,
            reserve0: r.reserve0.normalized().to_plain_string(),
            reserve1: r.reserve1.normalized().to_plain_string(),
            tvl_usd: r.pool_value_usd.normalized().to_plain_string(),
            total_supply: r.pool_total_supply.normalized().to_plain_string(),
            apr: apr.map(|v| format!("{:.4}", v)),
        },
        token0: PoolTokenSide {
            token_id: r.token0,
            symbol: token0_symbol,
            decimals: r.token0_decimals.unwrap_or(18),
            image_uri: r.token0_image.unwrap_or_default(),
        },
        token1: PoolTokenSide {
            token_id: r.token1,
            symbol: token1_symbol,
            decimals: r.token1_decimals.unwrap_or(18),
            image_uri: r.token1_image.unwrap_or_default(),
        },
        fee_config,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use std::str::FromStr;

    const POOL: &str = "0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01";
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02";

    async fn seed_pool(pool: &PgPool) {
        sqlx::query(
            r#"INSERT INTO pool (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                                 latest_trade_at, created_at, block_number, tx_hash, total_supply)
               VALUES ($1, $2, $3, 100000, 1500000, 15, 0, 1000.61, 0, 0, 1, '0x', 5000)"#,
        )
        .bind(POOL)
        .bind(TOKEN0)
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();

        for (id, sym) in [(TOKEN0, "MON"), (TOKEN1, "CHOG")] {
            sqlx::query(
                r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
                   VALUES ($1, $2, $3, 18, '', 0)"#,
            )
            .bind(id)
            .bind(sym)
            .bind(sym)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn seed_fee_config(pool: &PgPool) {
        sqlx::query(
            r#"INSERT INTO fee_config (pair_id, token_id, creator_fee_rate,
                                       curve_protocol_fee_rate, dex_protocol_fee_rate, created_at)
               VALUES ($1, $2, 10, 15, 25, 0)"#,
        )
        .bind(POOL)
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_controller(pool: PgPool) -> PoolController {
        PoolController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_returns_none_when_pool_missing(pool: PgPool) {
        let controller = make_controller(pool);
        let resp = controller.get_pool_detail(POOL).await.unwrap();
        assert!(
            resp.is_none(),
            "missing pool should return None (→ 404 in handler)"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_returns_pool_detail_without_fee_config(pool: PgPool) {
        seed_pool(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .get_pool_detail(POOL)
            .await
            .unwrap()
            .expect("pool exists");

        assert_eq!(resp.pool_info.pool_id, POOL);
        assert_eq!(resp.pool_info.pair_label, "MON-CHOG");
        assert_eq!(resp.token0.symbol, "MON");
        assert_eq!(resp.token0.decimals, 18);
        assert_eq!(resp.token1.symbol, "CHOG");
        assert_eq!(resp.pool_info.reserve0, "100000");
        assert_eq!(resp.pool_info.reserve1, "1500000");
        // tvl_usd from Postgres returns trailing zeros (e.g. "1000.6100" vs "1000.61").
        // Use BigDecimal comparison for robustness.
        let tvl = bigdecimal::BigDecimal::from_str(&resp.pool_info.tvl_usd).unwrap();
        let expected_tvl = bigdecimal::BigDecimal::from_str("1000.61").unwrap();
        assert_eq!(tvl, expected_tvl);
        assert_eq!(resp.pool_info.total_supply, "5000");
        assert!(resp.pool_info.apr.is_none(), "no pool_apr row → APR is None");
        assert!(
            resp.fee_config.is_none(),
            "no fee_config row → fee_config is None"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_returns_pool_detail_with_fee_config(pool: PgPool) {
        seed_pool(&pool).await;
        seed_fee_config(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .get_pool_detail(POOL)
            .await
            .unwrap()
            .expect("pool exists");

        let fc = resp.fee_config.expect("fee_config seeded");
        assert_eq!(fc.creator_bps, 10);
        assert_eq!(fc.curve_protocol_bps, 15);
        assert_eq!(fc.dex_protocol_bps, 25);
    }
}
