use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::tokens::{DexTokenEntry, DexTokenListQuery, DexTokenListResponse},
};

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    token_id: String,
    token_type: String,
    symbol: Option<String>,
    name: Option<String>,
    decimals: Option<i32>,
    image_uri: Option<String>,
    balance: Option<BigDecimal>,
    balance_usd: Option<BigDecimal>,
    market_cap_usd: Option<BigDecimal>,
    is_held: bool,
    tier: i32,
}

pub struct TokensController {
    db: Arc<PostgresDatabase>,
}

impl TokensController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn list_tokens(&self, query: &DexTokenListQuery) -> Result<DexTokenListResponse> {
        match query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => self.list_default(query).await,
            Some(q) => self.search(query, q).await,
        }
    }

    async fn list_default(&self, query: &DexTokenListQuery) -> Result<DexTokenListResponse> {
        let limit = query.limit;
        let offset = (query.page - 1) * limit;

        let total_count: i64 = measure_postgres!(
            "dex.list_tokens.count",
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT (SELECT COUNT(*) FROM whitelist_token WHERE enabled)
                     + (SELECT COUNT(*) FROM token t
                         WHERE t.version='V2'
                           AND EXISTS (SELECT 1 FROM pool p WHERE p.token0=t.token_id OR p.token1=t.token_id)
                           AND t.token_id NOT IN (SELECT token_id FROM whitelist_token WHERE enabled))
                "#,
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to count tokens: {}", e))?;

        let list_sql = format!("{ENRICHED_CTE}{ENRICHED_COLS}{ORDER_CLAUSE}LIMIT $2 OFFSET $3");
        let rows = measure_postgres!(
            "dex.list_tokens",
            sqlx::query_as::<_, TokenRow>(&list_sql)
                .bind(query.account.as_deref())
                .bind(limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to list tokens: {}", e))?;

        let tokens = rows.into_iter().map(|r| row_to_entry(r, query.account.is_some())).collect();
        Ok(DexTokenListResponse { tokens, total_count })
    }

    async fn search(&self, query: &DexTokenListQuery, q: &str) -> Result<DexTokenListResponse> {
        let limit = query.limit;
        let offset = (query.page - 1) * limit;
        match classify_query(q) {
            SearchKind::FullCa(ca) => self.search_full_ca(query, &ca).await,
            SearchKind::Text(s) => {
                self.run_filtered_search(
                    query.account.as_deref(),
                    &format!("{}%", s),
                    "WHERE (symbol ILIKE $2 OR name ILIKE $2)",
                    limit,
                    offset,
                )
                .await
            }
            SearchKind::PartialCa(s) => {
                self.run_filtered_search(
                    query.account.as_deref(),
                    &format!("{}%", s),
                    "WHERE token_id ILIKE $2",
                    limit,
                    offset,
                )
                .await
            }
        }
    }

    async fn run_filtered_search(
        &self,
        account: Option<&str>,
        pattern: &str,
        where_clause: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DexTokenListResponse> {
        let list_sql =
            format!("{ENRICHED_CTE}{ENRICHED_COLS}{where_clause}\n{ORDER_CLAUSE}LIMIT $3 OFFSET $4");
        let count_sql =
            format!("{ENRICHED_CTE}SELECT COUNT(*) FROM enriched {where_clause}");

        let total_count: i64 = measure_postgres!(
            "dex.search_tokens.count",
            sqlx::query_scalar::<_, i64>(&count_sql)
                .bind(account)
                .bind(pattern)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to count search: {}", e))?;

        let rows = measure_postgres!(
            "dex.search_tokens",
            sqlx::query_as::<_, TokenRow>(&list_sql)
                .bind(account)
                .bind(pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to search tokens: {}", e))?;

        let tokens = rows.into_iter().map(|r| row_to_entry(r, account.is_some())).collect();
        Ok(DexTokenListResponse { tokens, total_count })
    }

    async fn search_full_ca(
        &self,
        query: &DexTokenListQuery,
        ca: &str,
    ) -> Result<DexTokenListResponse> {
        let rows = measure_postgres!(
            "dex.search_full_ca",
            sqlx::query_as::<_, TokenRow>(SEARCH_FULL_CA_SQL)
                .bind(query.account.as_deref())
                .bind(ca)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed full CA search: {}", e))?;
        let total_count = rows.len() as i64;
        let tokens =
            rows.into_iter().map(|r| row_to_entry(r, query.account.is_some())).collect();
        Ok(DexTokenListResponse { tokens, total_count })
    }
}

// ---------------------------------------------------------------------------
// SQL consts — shared by default list and search paths
// ---------------------------------------------------------------------------

/// Candidate + enrichment CTEs. Uses $1 = account (nullable). No braces in SQL → safe for format!.
const ENRICHED_CTE: &str = r#"
WITH wl AS (
    SELECT token_id, sort_order FROM whitelist_token WHERE enabled
),
v2 AS (
    SELECT t.token_id
    FROM token t
    WHERE t.version='V2'
      AND EXISTS (SELECT 1 FROM pool p WHERE p.token0=t.token_id OR p.token1=t.token_id)
      AND t.token_id NOT IN (SELECT token_id FROM wl)
),
candidates AS (
    SELECT token_id, 'whitelist'::text AS token_type, sort_order FROM wl
    UNION ALL
    SELECT token_id, 'nadfun_v2'::text AS token_type, NULL::int AS sort_order FROM v2
),
enriched AS (
    SELECT
        c.token_id,
        c.token_type,
        c.sort_order,
        COALESCE(t.symbol,    dt.symbol,    qt.symbol)    AS symbol,
        COALESCE(t.name,      dt.name,      qt.name)      AS name,
        COALESCE(dt.decimals, qt.decimals)                AS decimals,
        COALESCE(t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
        b.balance AS balance,
        (b.balance / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC
            * m.price * lp.price) AS balance_usd,
        (m.price * (t.total_supply / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC)
            * lp.price) AS market_cap_usd,
        (b.balance IS NOT NULL AND b.balance > 0) AS is_held,
        CASE
          WHEN $1 IS NOT NULL AND b.balance > 0 AND c.token_type='whitelist' THEN 1
          WHEN $1 IS NOT NULL AND b.balance > 0 AND c.token_type='nadfun_v2' THEN 2
          WHEN c.token_type='whitelist' THEN 3
          ELSE 4
        END AS tier
    FROM candidates c
    LEFT JOIN token       t  ON t.token_id  = c.token_id
    LEFT JOIN dex_token   dt ON dt.token_id = c.token_id
    LEFT JOIN quote_token qt ON qt.quote_id = c.token_id
    LEFT JOIN balance     b  ON b.token_id  = c.token_id
                            AND b.account_id = COALESCE($1::VARCHAR, '__NO_ACCOUNT__')
    LEFT JOIN market      m  ON m.token_id  = c.token_id
    LEFT JOIN LATERAL (
        SELECT price FROM price WHERE quote_id = m.quote_id
        ORDER BY block_number DESC LIMIT 1
    ) lp ON true
)
"#;

const ENRICHED_COLS: &str = r#"SELECT token_id, token_type, symbol, name, decimals, image_uri,
       balance, balance_usd, market_cap_usd, is_held, tier
FROM enriched
"#;

/// ORDER BY clause referencing all columns from enriched (sort_order available even though not in SELECT).
const ORDER_CLAUSE: &str = r#"ORDER BY
    tier,
    CASE WHEN tier IN (1,2) THEN balance_usd END DESC NULLS LAST,
    CASE WHEN tier = 3 THEN sort_order END ASC NULLS LAST,
    CASE WHEN tier = 4 THEN market_cap_usd END DESC NULLS LAST,
    symbol ASC NULLS LAST,
    token_id ASC
"#;

/// Full-CA search — resolves a single exact token_id across ALL tables (incl. external).
/// $1 = account (nullable), $2 = exact token_id (EIP-55 checksum).
const SEARCH_FULL_CA_SQL: &str = r#"
WITH target AS (SELECT $2::VARCHAR AS token_id)
SELECT
    tg.token_id,
    CASE
      WHEN wl.token_id IS NOT NULL THEN 'whitelist'
      WHEN t.version = 'V2' THEN 'nadfun_v2'
      ELSE 'external'
    END AS token_type,
    COALESCE(t.symbol,    dt.symbol,    qt.symbol)    AS symbol,
    COALESCE(t.name,      dt.name,      qt.name)      AS name,
    COALESCE(dt.decimals, qt.decimals)                AS decimals,
    COALESCE(t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
    b.balance AS balance,
    (b.balance / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC * m.price * lp.price) AS balance_usd,
    (m.price * (t.total_supply / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC) * lp.price) AS market_cap_usd,
    (b.balance IS NOT NULL AND b.balance > 0) AS is_held,
    CASE
      WHEN $1 IS NOT NULL AND b.balance > 0 AND wl.token_id IS NOT NULL THEN 1
      WHEN $1 IS NOT NULL AND b.balance > 0 AND t.version='V2' THEN 2
      WHEN wl.token_id IS NOT NULL THEN 3
      ELSE 4
    END AS tier
FROM target tg
LEFT JOIN whitelist_token wl ON wl.token_id = tg.token_id AND wl.enabled
LEFT JOIN token       t  ON t.token_id  = tg.token_id
LEFT JOIN dex_token   dt ON dt.token_id = tg.token_id
LEFT JOIN quote_token qt ON qt.quote_id = tg.token_id
LEFT JOIN balance     b  ON b.token_id  = tg.token_id AND b.account_id = COALESCE($1::VARCHAR, '__NO_ACCOUNT__')
LEFT JOIN market      m  ON m.token_id  = tg.token_id
LEFT JOIN LATERAL (SELECT price FROM price WHERE quote_id=m.quote_id ORDER BY block_number DESC LIMIT 1) lp ON true
WHERE (t.token_id IS NOT NULL OR dt.token_id IS NOT NULL OR qt.quote_id IS NOT NULL)
"#;

// ---------------------------------------------------------------------------
// Query classification
// ---------------------------------------------------------------------------

enum SearchKind {
    Text(String),
    FullCa(String),
    PartialCa(String),
}

fn classify_query(q: &str) -> SearchKind {
    let t = q.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return SearchKind::FullCa(t.to_string());
        }
        return SearchKind::PartialCa(t.to_string());
    }
    SearchKind::Text(t.to_string())
}

// ---------------------------------------------------------------------------
// Row → response entry
// ---------------------------------------------------------------------------

fn row_to_entry(r: TokenRow, account_provided: bool) -> DexTokenEntry {
    DexTokenEntry {
        token_id: r.token_id,
        symbol: r.symbol.unwrap_or_default(),
        name: r.name.unwrap_or_default(),
        decimals: r.decimals.unwrap_or(18),
        image_uri: r.image_uri.unwrap_or_default(),
        is_external: r.token_type == "external",
        token_type: r.token_type,
        is_held: r.is_held,
        balance: if account_provided {
            Some(r.balance.unwrap_or_else(|| BigDecimal::from(0)).normalized().to_plain_string())
        } else {
            None
        },
        balance_usd: r.balance_usd.map(|v| v.normalized().to_plain_string()),
        market_cap_usd: r.market_cap_usd.map(|v| v.normalized().to_plain_string()),
        tier: r.tier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000aA11";
    const POOL: &str = "0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01"; // CHOG, launchpad
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02"; // WMON, dex_token only

    const WL_A: &str = "0x000000000000000000000000000000000000cC01"; // whitelist sort 1
    const WL_B: &str = "0x000000000000000000000000000000000000cC02"; // whitelist sort 2

    async fn seed_pool_with_two_tokens(pool: &PgPool) {
        // pool row referencing two token addresses
        sqlx::query(
            r#"INSERT INTO pool (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                                 latest_trade_at, created_at, block_number, tx_hash, total_supply)
               VALUES ($1, $2, $3, 100000, 1500000, 15, 0, 1000, 0, 0, 1, '0x', 5000)"#,
        )
        .bind(POOL)
        .bind(TOKEN0)
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();

        // TOKEN0: launchpad token (has total_supply for marketcap calc; needs an account creator).
        sqlx::query(
            r#"INSERT INTO account (account_id, nickname, bio, image_uri, follower_count, following_count)
               VALUES ($1, 'creator', '', '', 0, 0) ON CONFLICT DO NOTHING"#,
        )
        .bind(ACCOUNT)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO token (token_id, name, symbol, image_uri, creator, description,
                                  twitter, telegram, website, created_at,
                                  transaction_hash, total_supply)
               VALUES ($1, 'Chog Token', 'CHOG', '', $2, '', '', '', '', 0, '0x', 1000000000000000000000000)"#,
        )
        .bind(TOKEN0)
        .bind(ACCOUNT)
        .execute(pool)
        .await
        .unwrap();

        // TOKEN1: pure dex_token (no `token` row → external).
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'WMON', 'WMON', 18, '', 0)"#,
        )
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_balance(pool: &PgPool, token_id: &str, amount_wei: &str) {
        sqlx::query(
            r#"INSERT INTO balance (account_id, token_id, balance, created_at)
               VALUES ($1, $2, $3::NUMERIC, 0)"#,
        )
        .bind(ACCOUNT)
        .bind(token_id)
        .bind(amount_wei)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_whitelist(pool: &PgPool, token_id: &str, order: i32) {
        sqlx::query("INSERT INTO whitelist_token (token_id, sort_order) VALUES ($1, $2)")
            .bind(token_id).bind(order).execute(pool).await.unwrap();
    }

    fn make_controller(pool: PgPool) -> TokensController {
        use crate::db::postgres::PostgresDatabase;
        TokensController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    fn empty_query() -> DexTokenListQuery {
        DexTokenListQuery { account: None, q: None, page: 1, limit: 50 }
    }

    // -----------------------------------------------------------------------
    // Existing 8 tests
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations-test")]
    async fn lists_only_whitelist_and_nadfun_v2(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // Make CHOG a nadfun V2 candidate
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        // Only CHOG (nadfun V2) should appear; WMON (external, dex_token only) excluded
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.total_count, 1, "total_count reflects only whitelist+nadfun V2");

        let chog = resp
            .tokens
            .iter()
            .find(|t| t.symbol == "CHOG")
            .expect("CHOG present");
        assert_eq!(chog.token_id, TOKEN0);
        assert!(chog.balance.is_none(), "no ?account= → balance is None");
        assert!(!chog.is_external, "nadfun V2 is not external");

        assert!(
            resp.tokens.iter().all(|t| t.token_id != TOKEN1),
            "WMON (external) excluded from default list"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn includes_balance_when_account_provided(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // Make CHOG a nadfun V2 candidate
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // 1000 CHOG (18 dp)
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: Some(ACCOUNT.to_string()),
                q: None,
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();

        let chog = resp
            .tokens
            .iter()
            .find(|t| t.symbol == "CHOG")
            .expect("CHOG present");
        // BigDecimal::to_string() may emit scientific notation for large values;
        // compare as parsed BigDecimal for a canonical numeric equality check.
        let chog_balance: BigDecimal = chog
            .balance
            .as_deref()
            .expect("CHOG balance should be Some")
            .parse()
            .expect("balance is a valid decimal");
        let expected_balance: BigDecimal = "1000000000000000000000".parse().unwrap();
        assert_eq!(chog_balance, expected_balance, "CHOG balance mismatch");

        // WMON (external) should not appear in the default list
        assert!(
            resp.tokens.iter().all(|t| t.token_id != TOKEN1),
            "WMON (external) excluded from default list"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pagination_returns_total_count(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // Make CHOG a nadfun V2 candidate
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        // Add a whitelist token so total_count = 2
        seed_whitelist(&pool, WL_A, 1).await;
        let controller = make_controller(pool);

        // Page 1, limit 1 → 1 token returned, total_count = 2
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: None,
                page: 1,
                limit: 1,
            })
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.total_count, 2, "total_count = all matching rows");

        // Page 2, limit 1 → 1 token returned, total_count still = 2
        let resp2 = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: None,
                page: 2,
                limit: 1,
            })
            .await
            .unwrap();
        assert_eq!(resp2.tokens.len(), 1);
        assert_eq!(resp2.total_count, 2, "total_count consistent across pages");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn returns_empty_when_no_pools(pool: PgPool) {
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        assert!(resp.tokens.is_empty());
        assert_eq!(resp.total_count, 0);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pagination_out_of_range_page_returns_correct_total_count(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // Make CHOG a nadfun V2 candidate
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        // Add a whitelist token so total_count = 2
        seed_whitelist(&pool, WL_A, 1).await;
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: None,
                page: 99, // way past the end
                limit: 1,
            })
            .await
            .unwrap();
        assert!(resp.tokens.is_empty(), "page 99 of 2-token list is empty");
        assert_eq!(resp.total_count, 2, "total count still reflects all matches");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn default_no_account_orders_whitelist_then_v2(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1").bind(TOKEN0).execute(&pool).await.unwrap();
        seed_whitelist(&pool, WL_A, 1).await;
        seed_whitelist(&pool, WL_B, 2).await;
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        assert_eq!(resp.tokens[0].token_id, WL_A);
        assert_eq!(resp.tokens[0].tier, 3);
        assert_eq!(resp.tokens[1].token_id, WL_B);
        assert_eq!(resp.tokens[2].token_id, TOKEN0); // CHOG nadfun V2, tier4
        assert_eq!(resp.tokens[2].tier, 4);
        assert!(resp.tokens.iter().all(|t| t.token_id != TOKEN1), "external WMON 제외");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn default_with_account_orders_four_tiers(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1").bind(TOKEN0).execute(&pool).await.unwrap();
        seed_whitelist(&pool, WL_A, 1).await; // 미보유 화이트리스트 → tier3
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // CHOG 보유 → tier2
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&DexTokenListQuery {
            account: Some(ACCOUNT.to_string()), q: None, page: 1, limit: 50,
        }).await.unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).unwrap();
        assert_eq!(chog.tier, 2, "보유 nadfun V2 → tier2");
        assert!(chog.is_held);
        assert_eq!(chog.balance.as_deref(), Some("1000000000000000000000"));
        let wl = resp.tokens.iter().find(|t| t.token_id == WL_A).unwrap();
        assert_eq!(wl.tier, 3, "미보유 화이트리스트 → tier3");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn account_unheld_whitelist_sorted_by_sort_order(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // WL_A = ...cC01, WL_B = ...cC02. sort_order intentionally reversed vs token_id.
        seed_whitelist(&pool, WL_A, 2).await;
        seed_whitelist(&pool, WL_B, 1).await;
        let controller = make_controller(pool);
        // account provided, but both whitelist tokens unheld → tier 3, must sort by sort_order.
        let resp = controller.list_tokens(&DexTokenListQuery {
            account: Some(ACCOUNT.to_string()), q: None, page: 1, limit: 50,
        }).await.unwrap();
        let wl_ids: Vec<&str> = resp.tokens.iter()
            .filter(|t| t.tier == 3).map(|t| t.token_id.as_str()).collect();
        assert_eq!(wl_ids, vec![WL_B, WL_A], "미보유 화이트리스트는 account 있어도 sort_order 우선");
    }

    // -----------------------------------------------------------------------
    // Task 4 — external/V1 exclusion from default list
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations-test")]
    async fn external_and_v1_excluded_from_default(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        // No version bump → CHOG stays V1, WMON is external. Neither is a default candidate.
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        assert!(resp.tokens.is_empty(), "V1 + external만 있으면 기본 리스트 비어야 함");
        assert_eq!(resp.total_count, 0);
    }

    // -----------------------------------------------------------------------
    // Task 5 — search behaviors
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_text_prefix_only(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        let controller = make_controller(pool);
        let hit = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("CHO".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert_eq!(hit.tokens.len(), 1);
        assert_eq!(hit.tokens[0].token_id, TOKEN0);
        let miss = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("HOG".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(miss.tokens.is_empty(), "substring 매칭 금지 (prefix만)");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_exposes_external(pool: PgPool) {
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'Ext', 'EXT', 18, '', 0)"#,
        )
        .bind(TOKEN1)
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(TOKEN1.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].token_id, TOKEN1);
        assert_eq!(resp.tokens[0].token_type, "external");
        assert!(resp.tokens[0].is_external);
        assert_eq!(resp.tokens[0].tier, 4);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_partial_ca_excludes_external(pool: PgPool) {
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'Ext', 'EXT', 18, '', 0)"#,
        )
        .bind(TOKEN1)
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);
        let prefix = &TOKEN1[..10]; // partial CA (0x + 8 hex)
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(prefix.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(resp.tokens.is_empty(), "partial CA는 external 제외");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_nonexistent_returns_empty(pool: PgPool) {
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("0x000000000000000000000000000000000000dEaD".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(resp.tokens.is_empty(), "어느 테이블에도 없는 full CA → 빈 결과");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_text_excludes_external_symbol_match(pool: PgPool) {
        // external token (dex_token only) whose symbol matches the "CHO" prefix
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'Chogger', 'CHOGX', 18, '', 0)"#,
        )
        .bind(TOKEN1)
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("CHO".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(
            resp.tokens.iter().all(|t| t.token_id != TOKEN1),
            "text 검색은 symbol이 prefix 매칭돼도 external 제외"
        );
    }
}
