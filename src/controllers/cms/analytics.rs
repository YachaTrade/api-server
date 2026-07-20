use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::{BigDecimal, num_bigint::BigInt};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::cms::analytics::{
        ChesterRetentionResponse, ChesterRetentionRound, CreatorFeeResponse, NewUsersResponse,
        TopHeldToken, UserActivityResponse, UserRoiResponse,
    },
};

pub struct AnalyticsController {
    db: Arc<PostgresDatabase>,
}

#[derive(Debug, sqlx::FromRow)]
struct UserActivityStatsRow {
    total_users: i64,
    avg_pnl_usd: Option<BigDecimal>,
    avg_pnl_native: Option<BigDecimal>,
    avg_volume_usd: Option<BigDecimal>,
    avg_volume_native: Option<BigDecimal>,
}

#[derive(Debug, sqlx::FromRow)]
struct TopHeldTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    holder_count: i64,
    avg_balance: BigDecimal,
}

#[derive(Debug, sqlx::FromRow)]
struct RoiStatsRow {
    avg_roi_percent: Option<BigDecimal>,
    median_roi_percent: Option<BigDecimal>,
    positive_roi_count: i64,
    negative_roi_count: i64,
    total_users: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ChesterRetentionRow {
    from_round: i64,
    to_round: i64,
    from_participants: i64,
    returning_participants: i64,
}

impl AnalyticsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// 이탈 또는 활성 유저 분석
    /// is_churned=true: last_swap_at < cutoff (이탈)
    /// is_churned=false: last_swap_at >= cutoff (활성)
    pub async fn get_user_activity(
        &self,
        inactive_days: i64,
        limit: i64,
        is_churned: bool,
    ) -> Result<UserActivityResponse> {
        let cutoff_seconds = inactive_days * 86400;

        // cutoff을 절대값으로 변환 → last_swap_at 인덱스 활용 가능
        // churned: last_swap_at < cutoff_timestamp (오래전에 마지막 활동)
        // active: last_swap_at >= cutoff_timestamp (최근에 활동)
        let comparison = if is_churned { "<" } else { ">=" };

        // 단일 쿼리로 stats + top tokens를 각각 가져오되,
        // cutoff을 절대 timestamp로 변환하여 인덱스 사용
        let stats_query = format!(
            r#"
            WITH cutoff AS (
                SELECT EXTRACT(EPOCH FROM NOW())::BIGINT - $1 AS ts
            ),
            target_users AS (
                SELECT aa.account_id
                FROM account_activity aa, cutoff
                WHERE aa.last_swap_at {} cutoff.ts
            )
            SELECT
                COUNT(DISTINCT tu.account_id) as total_users,
                AVG(pa.realized_usd + pa.unrealized_usd) as avg_pnl_usd,
                AVG(pa.realized_native + pa.unrealized_native) as avg_pnl_native,
                AVG(vol.volume_usd) as avg_volume_usd,
                AVG(vol.volume_native) as avg_volume_native
            FROM target_users tu
            LEFT JOIN pnl_aggregator pa ON tu.account_id = pa.account_id
            LEFT JOIN (
                SELECT s.account_id,
                       SUM(s.value) as volume_usd,
                       SUM(s.quote_amount) as volume_native
                FROM swap s
                INNER JOIN (
                    SELECT aa.account_id
                    FROM account_activity aa, (SELECT EXTRACT(EPOCH FROM NOW())::BIGINT - $1 AS ts) c
                    WHERE aa.last_swap_at {} c.ts
                ) tu ON s.account_id = tu.account_id
                GROUP BY s.account_id
            ) vol ON tu.account_id = vol.account_id
            "#,
            comparison, comparison
        );

        let stats = measure_postgres!(
            "analytics.user_activity_stats",
            sqlx::query_as::<_, UserActivityStatsRow>(&stats_query)
                .bind(cutoff_seconds)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch user activity stats: {}", err))?;

        // Top 보유 토큰 — target_users와 먼저 JOIN하여 balance 스캔 범위 축소
        let tokens_query = format!(
            r#"
            WITH cutoff AS (
                SELECT EXTRACT(EPOCH FROM NOW())::BIGINT - $1 AS ts
            ),
            target_users AS (
                SELECT aa.account_id
                FROM account_activity aa, cutoff
                WHERE aa.last_swap_at {} cutoff.ts
            )
            SELECT
                b.token_id,
                COALESCE(t.name, '') as name,
                COALESCE(t.symbol, '') as symbol,
                COUNT(DISTINCT b.account_id) as holder_count,
                AVG(b.balance) as avg_balance
            FROM balance b
            INNER JOIN target_users tu ON b.account_id = tu.account_id
            LEFT JOIN token t ON b.token_id = t.token_id
            WHERE b.balance > 0
            GROUP BY b.token_id, t.name, t.symbol
            ORDER BY holder_count DESC
            LIMIT $2
            "#,
            comparison
        );

        let token_rows = measure_postgres!(
            "analytics.top_held_tokens",
            sqlx::query_as::<_, TopHeldTokenRow>(&tokens_query)
                .bind(cutoff_seconds)
                .bind(limit)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch top held tokens: {}", err))?;

        let top_held_tokens = token_rows
            .into_iter()
            .map(|row| TopHeldToken {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                holder_count: row.holder_count,
                avg_balance: row.avg_balance.round(2).to_string(),
            })
            .collect();

        Ok(UserActivityResponse {
            total_users: stats.total_users,
            avg_pnl_usd: stats.avg_pnl_usd.unwrap_or_default().round(2).to_string(),
            avg_pnl_native: stats
                .avg_pnl_native
                .unwrap_or_default()
                .round(2)
                .to_string(),
            avg_volume_usd: stats
                .avg_volume_usd
                .unwrap_or_default()
                .round(2)
                .to_string(),
            avg_volume_native: stats
                .avg_volume_native
                .unwrap_or_default()
                .round(2)
                .to_string(),
            top_held_tokens,
        })
    }

    /// 신규 유저 수 (첫 swap 기준)
    pub async fn get_new_users(&self, days: i64) -> Result<NewUsersResponse> {
        let cutoff_seconds = days * 86400;

        // 절대 timestamp 비교로 idx_account_activity_first_swap 인덱스 활용
        let count = measure_postgres!(
            "analytics.new_users",
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*) as count
                FROM account_activity
                WHERE first_swap_at >= EXTRACT(EPOCH FROM NOW())::BIGINT - $1
                "#
            )
            .bind(cutoff_seconds)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch new users count: {}", err))?;

        Ok(NewUsersResponse {
            new_users: count,
            period_days: days,
        })
    }

    /// 유저별 ROI 통계 — stats + median을 단일 쿼리로
    pub async fn get_user_roi(&self) -> Result<UserRoiResponse> {
        let stats = measure_postgres!(
            "analytics.user_roi_stats",
            sqlx::query_as::<_, RoiStatsRow>(
                r#"
                SELECT
                    AVG(
                        CASE WHEN total_invested_usd > 0
                        THEN ((realized_usd + unrealized_usd) / total_invested_usd) * 100
                        ELSE 0 END
                    ) as avg_roi_percent,
                    PERCENTILE_CONT(0.5) WITHIN GROUP (
                        ORDER BY CASE WHEN total_invested_usd > 0
                        THEN ((realized_usd + unrealized_usd) / total_invested_usd) * 100
                        ELSE 0 END
                    )::NUMERIC as median_roi_percent,
                    COUNT(CASE WHEN (realized_usd + unrealized_usd) > 0 THEN 1 END) as positive_roi_count,
                    COUNT(CASE WHEN (realized_usd + unrealized_usd) <= 0 THEN 1 END) as negative_roi_count,
                    COUNT(*) as total_users
                FROM pnl_aggregator
                WHERE total_invested_usd > 0
                "#
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch ROI stats: {}", err))?;

        Ok(UserRoiResponse {
            avg_roi_percent: stats
                .avg_roi_percent
                .unwrap_or_default()
                .round(2)
                .to_string(),
            median_roi_percent: stats
                .median_roi_percent
                .unwrap_or_default()
                .round(2)
                .to_string(),
            positive_roi_count: stats.positive_roi_count,
            negative_roi_count: stats.negative_roi_count,
            total_users: stats.total_users,
        })
    }

    /// 체스터 라운드간 리텐션
    pub async fn get_chester_retention(&self) -> Result<ChesterRetentionResponse> {
        let rows = measure_postgres!(
            "analytics.chester_retention",
            sqlx::query_as::<_, ChesterRetentionRow>(
                r#"
                WITH round_participants AS (
                    SELECT DISTINCT round, account_id
                    FROM chester_box_reward
                ),
                round_pairs AS (
                    SELECT
                        r1.round as from_round,
                        r2.round as to_round,
                        COUNT(DISTINCT r1.account_id) as from_participants,
                        COUNT(DISTINCT CASE WHEN r2.account_id IS NOT NULL THEN r1.account_id END) as returning_participants
                    FROM round_participants r1
                    LEFT JOIN round_participants r2
                        ON r1.account_id = r2.account_id
                        AND r2.round = r1.round + 1
                    GROUP BY r1.round, r2.round
                )
                SELECT
                    from_round,
                    from_round + 1 as to_round,
                    from_participants,
                    returning_participants
                FROM round_pairs
                WHERE to_round IS NOT NULL OR from_round = (SELECT MAX(round) FROM chester_box_reward)
                ORDER BY from_round
                "#
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch chester retention: {}", err))?;

        let rounds = rows
            .into_iter()
            .map(|row| {
                let rate = if row.from_participants > 0 {
                    row.returning_participants as f64 / row.from_participants as f64 * 100.0
                } else {
                    0.0
                };
                ChesterRetentionRound {
                    from_round: row.from_round,
                    to_round: row.to_round,
                    from_participants: row.from_participants,
                    returning_participants: row.returning_participants,
                    retention_rate: format!("{:.2}", rate),
                }
            })
            .collect();

        Ok(ChesterRetentionResponse { rounds })
    }

    /// 토큰 기간별 거래량/크리에이터 수수료 집계.
    /// 모든 token 행은 nadfun bonding-curve token이다. 토큰 미존재 시 `Ok(None)`.
    /// `to`는 배타적 상한(생략 시 상한 없음).
    /// 금액은 해당 토큰 quote token decimals(`market.quote_id` → `quote_token.decimals`)로 스케일.
    pub async fn creator_fee_stats(
        &self,
        token_id: &str,
        from: i64,
        to: Option<i64>,
    ) -> Result<Option<CreatorFeeResponse>> {
        // quote token 메타를 한 번에. token row가 없으면 Ok(None) = 404 신호.
        // 토큰당 market 1행(PK token_id)이므로 quote는 단일 → decimals/symbol 단일.
        // market/quote_token row가 없으면 native WMON(18 decimals)로 기본값.
        let meta: Option<TokenQuoteMetaRow> = measure_postgres!(
            "analytics.creator_fee.meta",
            sqlx::query_as::<_, TokenQuoteMetaRow>(
                r#"
                SELECT
                    COALESCE(qt.decimals, 18) AS decimals,
                    COALESCE(qt.symbol, 'MON') AS quote_symbol,
                    COALESCE(m.quote_id, '0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A') AS quote_id
                FROM token t
                LEFT JOIN market m ON m.token_id = t.token_id
                LEFT JOIN quote_token qt ON qt.quote_id = m.quote_id
                WHERE t.token_id = $1
                "#
            )
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token meta: {}", err))?;

        let Some(meta) = meta else {
            return Ok(None);
        };
        // total_volume includes V2_CURVE/V2_DEX swap rows.
        let total_volume = measure_postgres!(
            "analytics.creator_fee.volume",
            sqlx::query_scalar::<_, BigDecimal>(
                r#"
                SELECT COALESCE(SUM(quote_amount), 0)
                FROM swap
                WHERE token_id = $1 AND created_at >= $2
                  AND ($3::bigint IS NULL OR created_at < $3)
                "#
            )
            .bind(token_id)
            .bind(from)
            .bind(to)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch total volume: {}", err))?;

        let total_raw = measure_postgres!(
            "analytics.creator_fee.v2",
            sqlx::query_scalar::<_, BigDecimal>(
                r#"
                SELECT COALESCE(SUM(amount), 0)
                FROM v2_creator_fee_distribution
                WHERE token = $1 AND event_type = 'DISTRIBUTE'
                  AND created_at >= $2 AND ($3::bigint IS NULL OR created_at < $3)
                "#
            )
            .bind(token_id)
            .bind(from)
            .bind(to)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch v2 creator fee: {}", err))?;
        let pure_raw = BigDecimal::from(0);
        let sell_raw = BigDecimal::from(0);

        let decimals = meta.decimals;
        Ok(Some(CreatorFeeResponse {
            quote_id: meta.quote_id,
            quote_symbol: meta.quote_symbol,
            decimals,
            total_volume: scale_amount(&total_volume, decimals),
            pure_creator_fee: scale_amount(&pure_raw, decimals),
            sell_fee: scale_amount(&sell_raw, decimals),
            total_creator_fee: scale_amount(&total_raw, decimals),
        }))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TokenQuoteMetaRow {
    decimals: i32,
    quote_symbol: String,
    quote_id: String,
}

/// raw amount(BigDecimal)를 quote token 단위 문자열로. 10^decimals로 나눈 뒤
/// 끝자리 0 제거, 지수표기 없음. decimals는 quote_token.decimals (보통 18, USDC=6 등).
fn scale_amount(raw: &BigDecimal, decimals: i32) -> String {
    // 10^decimals = mantissa 1, scale -decimals (BigDecimal::new(m, s) = m * 10^-s)
    let divisor = BigDecimal::new(BigInt::from(1), -(decimals as i64));
    (raw.clone() / divisor).normalized().to_plain_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    // 체크섬 정규화된 주소 (handler가 valid_token_id로 변환 후 넘기는 형태와 동일)
    const T_V1: &str = "0x25d5934Ce90228B9B7cE03999b81069ab6297777";
    const T_V2: &str = "0x000000000000000000000000000000000000dEaD";
    const ACC: &str = "0x1111111111111111111111111111111111111111";

    fn make_controller(pool: PgPool) -> AnalyticsController {
        AnalyticsController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_token(pool: &PgPool, token_id: &str) {
        sqlx::query(
            r#"INSERT INTO token
               (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply)
               VALUES ($1, 'n', 's', 'u', $2, 0, $3, 0)
               ON CONFLICT (token_id) DO NOTHING"#,
        )
        .bind(token_id)
        .bind(ACC)
        .bind(format!("txtok_{token_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_swap(
        pool: &PgPool,
        token_id: &str,
        quote_amount: &str,
        created_at: i64,
        tx: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO swap
               (account_id, token_id, market_type, is_buy, quote_amount, token_amount,
                created_at, transaction_hash, tx_index, log_index)
               VALUES ($1, $2, 'DEX', true, $3::NUMERIC, 0, $4, $5, 0, 0)"#,
        )
        .bind(ACC)
        .bind(token_id)
        .bind(quote_amount)
        .bind(created_at)
        .bind(tx)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_v2_dist(
        pool: &PgPool,
        token: &str,
        event_type: &str,
        amount: &str,
        created_at: i64,
        tx: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO v2_creator_fee_distribution
               (event_type, token, amount, transaction_hash, block_number, created_at, log_index, tx_index)
               VALUES ($1, $2, $3::NUMERIC, $4, 0, $5, 0, 0)"#,
        )
        .bind(event_type)
        .bind(token)
        .bind(amount)
        .bind(tx)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_quote_token(pool: &PgPool, quote_id: &str, symbol: &str, decimals: i32) {
        sqlx::query(
            r#"INSERT INTO quote_token (quote_id, name, symbol, decimals, pyth_feed_id, image_uri)
               VALUES ($1, $2, $3, $4, 'feed', 'uri')
               ON CONFLICT (quote_id) DO UPDATE SET decimals = EXCLUDED.decimals, symbol = EXCLUDED.symbol"#,
        )
        .bind(quote_id)
        .bind(symbol)
        .bind(symbol)
        .bind(decimals)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_market(pool: &PgPool, token_id: &str, quote_id: &str) {
        sqlx::query(
            r#"INSERT INTO market (market_type, token_id, price, quote_id, latest_trade_at, created_at)
               VALUES ('V2_DEX', $1, 1, $2, 0, 0)
               ON CONFLICT (token_id) DO UPDATE SET quote_id = EXCLUDED.quote_id"#,
        )
        .bind(token_id)
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// n MON을 wei 문자열로 (n * 10^18)
    fn mon(n: u64) -> String {
        format!("{n}000000000000000000")
    }

    // USDC mainnet 주소 (유효한 EIP-55 체크섬), decimals 6
    const USDC: &str = "0xA0b86991c6218B36c1D19d4A2E9eB0cE3606eB48";

    #[sqlx::test(migrations = "./migrations-test")]
    async fn applies_quote_token_decimals(pool: PgPool) {
        // V2 토큰이 USDC(6 decimals) quote 사용 → /10^6로 스케일되어야 함
        seed_token(&pool, T_V2).await;
        seed_quote_token(&pool, USDC, "USDC", 6).await;
        seed_market(&pool, T_V2, USDC).await;
        seed_swap(&pool, T_V2, "1000000", 1000, "s1").await; // 1e6 raw = 1.0 USDC
        seed_v2_dist(&pool, T_V2, "DISTRIBUTE", "2000000", 1000, "v1").await; // 2e6 = 2.0

        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V2, 0, None)
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "1", "scaled by 10^6, not 10^18");
        assert_eq!(resp.total_creator_fee, "2", "scaled by 10^6, not 10^18");
        assert_eq!(resp.decimals, 6);
        assert_eq!(resp.quote_symbol, "USDC");
        assert_eq!(resp.quote_id, USDC);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn every_token_uses_distribution_table_and_zeros_pure_sell(pool: PgPool) {
        seed_token(&pool, T_V1).await;
        // volume: 2 + 3 = 5 MON
        seed_swap(&pool, T_V1, &mon(2), 1000, "s1").await;
        seed_swap(&pool, T_V1, &mon(3), 2000, "s2").await;
        seed_v2_dist(&pool, T_V1, "DISTRIBUTE", &mon(3), 1500, "d1").await;

        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V1, 0, None)
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "5");
        assert_eq!(resp.pure_creator_fee, "0");
        assert_eq!(resp.sell_fee, "0");
        assert_eq!(resp.total_creator_fee, "3");
        assert_eq!(resp.decimals, 18, "no market row → native 18 decimals");
        assert_eq!(resp.quote_symbol, "MON");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn v2_uses_distribution_table_and_zeros_pure_sell(pool: PgPool) {
        seed_token(&pool, T_V2).await;
        // volume still from swap: 10 MON
        seed_swap(&pool, T_V2, &mon(10), 1000, "s1").await;
        // creator fee from DISTRIBUTE events: 3 + 2 = 5 MON
        seed_v2_dist(&pool, T_V2, "DISTRIBUTE", &mon(3), 1000, "v1").await;
        seed_v2_dist(&pool, T_V2, "DISTRIBUTE", &mon(2), 2000, "v2").await;
        // CallbackFail must be excluded
        seed_v2_dist(&pool, T_V2, "CallbackFail", &mon(100), 1500, "v3").await;
        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V2, 0, None)
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "10");
        assert_eq!(resp.pure_creator_fee, "0", "v2 has no pure/sell split");
        assert_eq!(resp.sell_fee, "0");
        assert_eq!(
            resp.total_creator_fee, "5",
            "only DISTRIBUTE amounts, CallbackFail excluded"
        );
        assert_eq!(resp.decimals, 18, "no market row → native 18 decimals");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn respects_time_range_window(pool: PgPool) {
        seed_token(&pool, T_V1).await;
        seed_swap(&pool, T_V1, &mon(1), 1000, "s1").await; // < from → excluded
        seed_swap(&pool, T_V1, &mon(2), 2000, "s2").await; // == from → included
        seed_swap(&pool, T_V1, &mon(4), 3000, "s3").await; // in range → included
        seed_swap(&pool, T_V1, &mon(8), 4000, "s4").await; // == to → excluded (exclusive)

        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V1, 2000, Some(4000))
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "6", "[2000, 4000): 2 + 4");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn omitted_to_has_no_upper_bound(pool: PgPool) {
        seed_token(&pool, T_V1).await;
        seed_swap(&pool, T_V1, &mon(1), 1000, "s1").await; // < from → excluded
        seed_swap(&pool, T_V1, &mon(2), 2000, "s2").await;
        seed_swap(&pool, T_V1, &mon(4), 9999, "s3").await;

        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V1, 2000, None)
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "6", "from=2000, no upper bound: 2 + 4");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn returns_zeros_when_no_data(pool: PgPool) {
        seed_token(&pool, T_V1).await;

        let ctrl = make_controller(pool);
        let resp = ctrl
            .creator_fee_stats(T_V1, 0, None)
            .await
            .unwrap()
            .expect("token exists");

        assert_eq!(resp.total_volume, "0");
        assert_eq!(resp.pure_creator_fee, "0");
        assert_eq!(resp.sell_fee, "0");
        assert_eq!(resp.total_creator_fee, "0");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn returns_none_when_token_missing(pool: PgPool) {
        let ctrl = make_controller(pool);
        let resp = ctrl.creator_fee_stats(T_V1, 0, None).await.unwrap();
        assert!(
            resp.is_none(),
            "missing token → Ok(None) → handler maps to 404"
        );
    }
}
