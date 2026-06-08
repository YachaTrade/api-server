use std::sync::Arc;

use anyhow::Result;
use bigdecimal::{BigDecimal, Zero};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::pool_info::PoolInfo,
    types::dex::position::{LpPositionEntry, LpPositionTokenSide, LpPositionsResponse},
};

/// APR percent for one window. None when fee/tvl missing or tvl <= 0.
/// Inputs are LP-NET (post-0.8 carve-out) USD values from `pool_apr` view.
pub(crate) fn window_apr_pct(fee_usd: Option<f64>, tvl_usd_avg: Option<f64>, window_days: f64) -> Option<f64> {
    let fee = fee_usd?;
    let tvl = tvl_usd_avg?;
    if tvl <= 0.0 {
        return None;
    }
    Some((fee / tvl) * (365.0 / window_days) * 100.0)
}

/// Max APR across 24h, 7d, 30d windows. None when ALL three are undefined.
/// Also used by `controllers::dex::pool::row_to_response`.
pub(crate) fn apr_max_pct(
    fee_24h: Option<f64>, tvl_24h: Option<f64>,
    fee_7d: Option<f64>,  tvl_7d: Option<f64>,
    fee_30d: Option<f64>, tvl_30d: Option<f64>,
) -> Option<f64> {
    [
        window_apr_pct(fee_24h, tvl_24h, 1.0),
        window_apr_pct(fee_7d,  tvl_7d,  7.0),
        window_apr_pct(fee_30d, tvl_30d, 30.0),
    ]
    .into_iter()
    .flatten()
    .reduce(f64::max)
}

/// `my_liquidity_usd = balance × pool.value / pool.total_supply`, a USD value
/// truncated to 8 decimal places.
/// `None` when `total_supply <= 0` (defensive — pool has no LP at all).
///
/// The 8dp truncation matters: BigDecimal `/` keeps ~100 fractional digits on
/// non-even division, so without it the USD value serializes with a meaningless
/// long decimal tail (e.g. `"0.004065253230704521…337854"`).
pub(crate) fn my_liquidity_usd(
    balance: &BigDecimal,
    pool_value_usd: &BigDecimal,
    pool_total_supply: &BigDecimal,
) -> Option<BigDecimal> {
    if pool_total_supply.is_zero()
        || pool_total_supply.sign() == bigdecimal::num_bigint::Sign::Minus
    {
        return None;
    }
    Some(
        (balance * pool_value_usd / pool_total_supply)
            .with_scale_round(8, bigdecimal::RoundingMode::Down),
    )
}

#[derive(Debug, sqlx::FromRow)]
struct PositionRow {
    pool_id: String,
    token0: String,
    token1: String,
    balance: BigDecimal,
    deposited_token0: BigDecimal,
    deposited_token1: BigDecimal,
    deposited_token0_usd: BigDecimal,
    deposited_token1_usd: BigDecimal,
    pool_reserve0: BigDecimal,
    pool_reserve1: BigDecimal,
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
}

pub struct PositionController {
    db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_positions(&self, account_id: &str) -> Result<LpPositionsResponse> {
        let rows = measure_postgres!(
            "dex.get_positions",
            sqlx::query_as::<_, PositionRow>(
                r#"
                SELECT
                    lp.pool_id,
                    p.token0,
                    p.token1,
                    lp.balance,
                    (lp.token0_in     - lp.token0_out)     AS deposited_token0,
                    (lp.token1_in     - lp.token1_out)     AS deposited_token1,
                    (lp.token0_in_usd - lp.token0_out_usd) AS deposited_token0_usd,
                    (lp.token1_in_usd - lp.token1_out_usd) AS deposited_token1_usd,
                    p.reserve0     AS pool_reserve0,
                    p.reserve1     AS pool_reserve1,
                    p.value        AS pool_value_usd,
                    p.total_supply AS pool_total_supply,
                    COALESCE(wl0.symbol,    t0.symbol,    dt0.symbol,    qt0.symbol)    AS token0_symbol,
                    COALESCE(wl0.decimals,  dt0.decimals, qt0.decimals)                 AS token0_decimals,
                    COALESCE(wl0.image_uri, t0.image_uri, dt0.image_uri, qt0.image_uri) AS token0_image,
                    COALESCE(wl1.symbol,    t1.symbol,    dt1.symbol,    qt1.symbol)    AS token1_symbol,
                    COALESCE(wl1.decimals,  dt1.decimals, qt1.decimals)                 AS token1_decimals,
                    COALESCE(wl1.image_uri, t1.image_uri, dt1.image_uri, qt1.image_uri) AS token1_image,
                    par.lp_fee_24h_usd::float8  AS lp_fee_24h_usd,
                    par.tvl_24h_usd_avg::float8 AS tvl_24h_usd_avg,
                    par.lp_fee_7d_usd::float8   AS lp_fee_7d_usd,
                    par.tvl_7d_usd_avg::float8  AS tvl_7d_usd_avg,
                    par.lp_fee_30d_usd::float8  AS lp_fee_30d_usd,
                    par.tvl_30d_usd_avg::float8 AS tvl_30d_usd_avg
                FROM lp_position lp
                JOIN pool p
                    ON p.pool_id = lp.pool_id
                LEFT JOIN token          t0  ON t0.token_id  = p.token0
                LEFT JOIN dex_token      dt0 ON dt0.token_id = p.token0
                LEFT JOIN quote_token    qt0 ON qt0.quote_id = p.token0
                LEFT JOIN whitelist_token wl0 ON wl0.token_id = p.token0 AND wl0.enabled
                LEFT JOIN token          t1  ON t1.token_id  = p.token1
                LEFT JOIN dex_token      dt1 ON dt1.token_id = p.token1
                LEFT JOIN quote_token    qt1 ON qt1.quote_id = p.token1
                LEFT JOIN whitelist_token wl1 ON wl1.token_id = p.token1 AND wl1.enabled
                LEFT JOIN pool_apr       par ON par.pool_id  = p.pool_id
                WHERE lp.account_id = $1
                  AND lp.balance > 0
                ORDER BY lp.pool_id
                "#,
            )
            .bind(account_id)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to fetch positions: {}", e))?;

        let positions = rows.into_iter().map(row_to_entry).collect();
        Ok(LpPositionsResponse {
            account_id: account_id.to_string(),
            positions,
        })
    }
}

fn row_to_entry(r: PositionRow) -> LpPositionEntry {
    let token0_symbol = r.token0_symbol.unwrap_or_default();
    let token1_symbol = r.token1_symbol.unwrap_or_default();
    let pair_label = format!("{}-{}", token0_symbol, token1_symbol);

    let my_liq = my_liquidity_usd(&r.balance, &r.pool_value_usd, &r.pool_total_supply);
    let apr = apr_max_pct(
        r.lp_fee_24h_usd, r.tvl_24h_usd_avg,
        r.lp_fee_7d_usd,  r.tvl_7d_usd_avg,
        r.lp_fee_30d_usd, r.tvl_30d_usd_avg,
    );

    // Per-side current pro-rata share = balance × reserve / total_supply.
    // NULL when total_supply = 0 (guards divide-by-zero, mirrors my_liquidity_usd).
    let current_token0 = current_share(&r.balance, &r.pool_reserve0, &r.pool_total_supply);
    let current_token1 = current_share(&r.balance, &r.pool_reserve1, &r.pool_total_supply);

    LpPositionEntry {
        pool_info: PoolInfo {
            pool_id: r.pool_id,
            pair_label,
            reserve0: r.pool_reserve0.normalized().to_plain_string(),
            reserve1: r.pool_reserve1.normalized().to_plain_string(),
            tvl_usd: r.pool_value_usd.normalized().to_plain_string(),
            total_supply: r.pool_total_supply.normalized().to_plain_string(),
            apr: apr.map(|v| format!("{:.4}", v)),
        },
        token0: LpPositionTokenSide {
            token_id: r.token0,
            symbol: token0_symbol,
            decimals: r.token0_decimals.unwrap_or(18),
            image_uri: r.token0_image.unwrap_or_default(),
            deposit_amount: r.deposited_token0.normalized().to_plain_string(),
            deposit_usd: r.deposited_token0_usd.normalized().to_plain_string(),
            current_amount: current_token0.map(|v| v.normalized().to_plain_string()),
        },
        token1: LpPositionTokenSide {
            token_id: r.token1,
            symbol: token1_symbol,
            decimals: r.token1_decimals.unwrap_or(18),
            image_uri: r.token1_image.unwrap_or_default(),
            deposit_amount: r.deposited_token1.normalized().to_plain_string(),
            deposit_usd: r.deposited_token1_usd.normalized().to_plain_string(),
            current_amount: current_token1.map(|v| v.normalized().to_plain_string()),
        },
        balance: r.balance.normalized().to_plain_string(),
        liquidity_usd: my_liq.map(|v| v.normalized().to_plain_string()),
    }
}

/// Per-token current pro-rata share = floor(balance × reserve / total_supply).
/// None when total_supply <= 0.
///
/// Result is **raw wei (integer base units)** — there is no fractional wei, so we
/// floor to a whole number. Without this, BigDecimal `/` keeps ~100 fractional
/// digits on non-even division and the amount serializes with a meaningless long
/// decimal tail (e.g. `"1219326312467611623.8256…"`).
pub(crate) fn current_share(balance: &BigDecimal, reserve: &BigDecimal, total_supply: &BigDecimal) -> Option<BigDecimal> {
    if total_supply.is_zero()
        || total_supply.sign() == bigdecimal::num_bigint::Sign::Minus
    {
        return None;
    }
    // RoundingMode::Down = truncate toward zero = floor for non-negative values.
    Some((balance * reserve / total_supply).with_scale_round(0, bigdecimal::RoundingMode::Down))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn window_apr_pct_7d_baseline_130pct() {
        // 130% APR means weekly return of 130 / (365/7) / 100 = 2.49315068...%.
        // Inversely: fee=2.49315068% of TVL → APR = 130.
        let fee = 24.9315068493;
        let tvl = 1000.0;
        let apr = window_apr_pct(Some(fee), Some(tvl), 7.0).unwrap();
        assert!((apr - 130.0).abs() < 1e-6, "got {}", apr);
    }

    #[test]
    fn window_apr_pct_none_when_fee_missing() {
        assert!(window_apr_pct(None, Some(1000.0), 7.0).is_none());
    }

    #[test]
    fn window_apr_pct_none_when_tvl_missing() {
        assert!(window_apr_pct(Some(10.0), None, 7.0).is_none());
    }

    #[test]
    fn window_apr_pct_none_when_tvl_zero() {
        assert!(window_apr_pct(Some(10.0), Some(0.0), 7.0).is_none());
    }

    #[test]
    fn window_apr_pct_none_when_tvl_negative() {
        // pool.value should never go negative, but guard anyway.
        assert!(window_apr_pct(Some(10.0), Some(-1.0), 7.0).is_none());
    }

    #[test]
    fn apr_max_pct_picks_largest_window() {
        // 24h returns highest APR when 24h window has higher fee/tvl ratio
        let r = apr_max_pct(
            Some(10.0), Some(100.0),   // 24h: 10/100 * (365/1) * 100 = 3650
            Some(10.0), Some(1000.0),  // 7d: 10/1000 * 365/7 * 100 ≈ 521
            Some(10.0), Some(10000.0), // 30d: 10/10000 * 365/30 * 100 ≈ 12.17
        )
        .unwrap();
        assert!((r - 3650.0).abs() < 1e-6, "got {}", r);
    }

    #[test]
    fn apr_max_pct_skips_missing_windows() {
        // Only 7d has data → returns 7d APR
        let r = apr_max_pct(
            None, None,
            Some(24.9315068493), Some(1000.0), // ≈ 130 (baseline)
            None, None,
        )
        .unwrap();
        assert!((r - 130.0).abs() < 1e-6, "got {}", r);
    }

    #[test]
    fn apr_max_pct_none_when_all_windows_missing() {
        assert!(apr_max_pct(None, None, None, None, None, None).is_none());
    }

    #[test]
    fn my_liquidity_usd_basic() {
        let bal = BigDecimal::from(50);
        let value = BigDecimal::from(1000);
        let supply = BigDecimal::from(100);
        // 50 * 1000 / 100 = 500
        let got = my_liquidity_usd(&bal, &value, &supply).unwrap();
        assert_eq!(got, BigDecimal::from(500));
    }

    #[test]
    fn my_liquidity_usd_truncates_to_8dp() {
        use std::str::FromStr;
        // Live case (api.nad.fun MON-USDC position): balance × tvl / total_supply
        // is non-even → BigDecimal division would keep ~100 fractional digits.
        let got = my_liquidity_usd(
            &BigDecimal::from_str("13947259859").unwrap(),
            &BigDecimal::from_str("2.5506091973").unwrap(),
            &BigDecimal::from_str("8750748663038").unwrap(),
        )
        .unwrap();
        let s = got.normalized().to_plain_string();
        let frac = s.split('.').nth(1).map(|f| f.len()).unwrap_or(0);
        assert!(frac <= 8, "liquidity_usd must be <= 8 decimals, got '{}' ({} dp)", s, frac);
    }

    #[test]
    fn my_liquidity_usd_none_on_zero_supply() {
        let bal = BigDecimal::from(50);
        let value = BigDecimal::from(1000);
        let supply = BigDecimal::from(0);
        assert!(my_liquidity_usd(&bal, &value, &supply).is_none());
    }

    #[test]
    fn my_liquidity_usd_handles_decimal_precision() {
        let bal = BigDecimal::from_str("1234567890123456789").unwrap();
        let value = BigDecimal::from_str("5000.5").unwrap();
        let supply = BigDecimal::from_str("9876543210987654321").unwrap();
        let got = my_liquidity_usd(&bal, &value, &supply).unwrap();
        // Expected ≈ 1234567890123456789 × 5000.5 / 9876543210987654321 ≈ 625.0625
        // python3: 1234567890123456789 * 5000.5 / 9876543210987654321 = 625.0624943...
        // Tight bounds catch numerator/denominator swap, order-of-magnitude bugs,
        // sign errors, and precision blowups while tolerating BigDecimal scale variation.
        let lo = BigDecimal::from_str("625.0").unwrap();
        let hi = BigDecimal::from_str("625.1").unwrap();
        assert!(got > lo, "got {} < lo {}", got, lo);
        assert!(got < hi, "got {} > hi {}", got, hi);
    }

    #[test]
    fn current_share_floors_to_integer_raw_wei() {
        use std::str::FromStr;
        // Non-even division at realistic raw-wei scale. current_amount is raw wei
        // (integer base units) — there is no fractional wei, so the result must be
        // a whole number, NOT a ~100-digit BigDecimal-division tail.
        let got = current_share(
            &BigDecimal::from_str("1234567890123456789").unwrap(),
            &BigDecimal::from_str("987654321987654321").unwrap(),
            &BigDecimal::from_str("1000000000000000007").unwrap(),
        )
        .unwrap();
        let s = got.normalized().to_plain_string();
        assert!(
            !s.contains('.'),
            "current_amount must be integer raw wei (no fractional tail), got '{}'",
            s
        );
        assert_eq!(s, "1219326312467611623", "floored integer raw wei");
    }

    #[test]
    fn current_share_basic_and_zero_supply() {
        use bigdecimal::BigDecimal;
        use std::str::FromStr;
        let bal = BigDecimal::from_str("10").unwrap();
        let res = BigDecimal::from_str("1000").unwrap();
        let ts  = BigDecimal::from_str("100").unwrap();
        // 10 * 1000 / 100 = 100
        assert_eq!(current_share(&bal, &res, &ts), Some(BigDecimal::from_str("100").unwrap()));
        // total_supply 0 -> None (guard)
        assert_eq!(current_share(&bal, &res, &BigDecimal::from(0)), None);
    }

    // ----- Integration tests (require DATABASE_URL → real Postgres) -----

    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000aA11";
    const POOL_ADDR: &str = "0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01";
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02";

    /// Seed pool, dex_token rows, and one open lp_position row.
    ///
    /// Inserts directly into `lp_position` (bypasses triggers) so the test
    /// controls exact `token0_in`/`token1_in` values. The `balance` column
    /// is GENERATED ALWAYS AS (lp_in - lp_out) STORED so it is excluded
    /// from the INSERT; `lp_in = 50, lp_out = 0` → balance = 50.
    async fn seed_pool_with_position(pool: &PgPool) {
        sqlx::query(
            r#"INSERT INTO pool
                   (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                    latest_trade_at, created_at, block_number, tx_hash, total_supply)
               VALUES ($1, $2, $3, 100000, 200000, 2, 0, 1000.61, 0, 0, 1, '0x', 5000)"#,
        )
        .bind(POOL_ADDR)
        .bind(TOKEN0)
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();

        for (id, sym) in [(TOKEN0, "CHOG"), (TOKEN1, "WMON")] {
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

        // Insert directly into lp_position (bypasses triggers).
        // balance is a GENERATED column — must NOT be included in the column list.
        sqlx::query(
            r#"INSERT INTO lp_position
                   (account_id, pool_id,
                    lp_in, lp_out,
                    token0_in, token0_out, token1_in, token1_out,
                    token0_in_usd, token0_out_usd, token1_in_usd, token1_out_usd,
                    created_at, updated_at,
                    epoch_start_block, epoch_start_tx_index, epoch_start_log_index)
               VALUES ($1, $2,
                       50, 0,
                       2500, 0, 100, 0,
                       0, 0, 0, 0,
                       0, 0, 0, 0, 0)"#,
        )
        .bind(ACCOUNT)
        .bind(POOL_ADDR)
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_controller(pool: PgPool) -> PositionController {
        PositionController::new(std::sync::Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_returns_empty_when_no_positions(pool: PgPool) {
        let controller = make_controller(pool);
        let resp = controller.get_positions(ACCOUNT).await.unwrap();
        assert_eq!(resp.account_id, ACCOUNT);
        assert!(resp.positions.is_empty());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_returns_open_position_with_correct_fields(pool: PgPool) {
        seed_pool_with_position(&pool).await;
        let controller = make_controller(pool);
        let resp = controller.get_positions(ACCOUNT).await.unwrap();

        assert_eq!(resp.positions.len(), 1);
        let p = &resp.positions[0];
        assert_eq!(p.pool_info.pool_id, POOL_ADDR);
        assert_eq!(p.pool_info.pair_label, "CHOG-WMON");
        assert_eq!(p.token0.symbol, "CHOG");
        assert_eq!(p.token0.deposit_amount, "2500");
        assert_eq!(p.token1.symbol, "WMON");
        assert_eq!(p.token1.deposit_amount, "100");
        assert_eq!(p.balance, "50");
        // Compare as BigDecimal to tolerate trailing zeros added by Postgres NUMERIC.
        assert_eq!(
            BigDecimal::from_str(&p.pool_info.tvl_usd).unwrap(),
            BigDecimal::from_str("1000.61").unwrap(),
            "tvl_usd mismatch: got {}",
            p.pool_info.tvl_usd
        );
        // my_liquidity_usd = 50 × 1000.61 / 5000 = 10.0061
        let my_liq_str = p.liquidity_usd.as_deref().unwrap();
        let my_liq = BigDecimal::from_str(my_liq_str).unwrap();
        let expected = BigDecimal::from_str("10.0061").unwrap();
        assert_eq!(my_liq, expected, "got {}", my_liq_str);
        // No pool_apr row seeded → apr null
        assert!(p.pool_info.apr.is_none());
        // Current pro-rata share: balance × reserve / total_supply
        // 50 × 100000 / 5000 = 1000, 50 × 200000 / 5000 = 2000
        assert_eq!(p.token0.current_amount.as_deref(), Some("1000"));
        assert_eq!(p.token1.current_amount.as_deref(), Some("2000"));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn integration_excludes_closed_positions(pool: PgPool) {
        seed_pool_with_position(&pool).await;
        // Close position by setting lp_out = lp_in (balance = 0)
        sqlx::query("UPDATE lp_position SET lp_out = lp_in WHERE account_id = $1")
            .bind(ACCOUNT)
            .execute(&pool)
            .await
            .unwrap();
        let controller = make_controller(pool);
        let resp = controller.get_positions(ACCOUNT).await.unwrap();
        assert!(
            resp.positions.is_empty(),
            "closed position should be filtered out"
        );
    }
}
