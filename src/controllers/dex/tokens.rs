use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    services::pricing::{
        balance::RpcBalanceSource, compute_balance_usd, meta::RpcMetaSource, BalanceSource,
        TokenMetaSource,
    },
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
    /// USD unit price from SQL: dex_token_price → market.price × quote→USD →
    /// price_usd table (DefiLlama; covers whitelist which has no market row).
    price_usd: Option<BigDecimal>,
}

pub struct TokensController {
    db: Arc<PostgresDatabase>,
    balance_source: Arc<dyn BalanceSource>,
    /// full-CA 미발견 시 온체인 메타 fallback. None이면 fallback 없음(미발견 → 빈 결과).
    meta_source: Option<Arc<dyn TokenMetaSource>>,
}

impl TokensController {
    /// 운영용 — 실제 RPC 소스. 가격은 SQL(price_usd 테이블)에서 읽음.
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self {
            db,
            balance_source: Arc::new(RpcBalanceSource::new()),
            meta_source: Some(Arc::new(RpcMetaSource::new())),
        }
    }

    /// 테스트용 — 소스 주입. meta fallback은 기본 비활성(미발견 → 빈 결과).
    pub fn with_sources(
        db: Arc<PostgresDatabase>,
        balance_source: Arc<dyn BalanceSource>,
    ) -> Self {
        Self { db, balance_source, meta_source: None }
    }

    /// 테스트용 — full-CA 미발견 시 외부 토큰 메타 fallback 주입.
    pub fn with_meta_source(mut self, meta_source: Arc<dyn TokenMetaSource>) -> Self {
        self.meta_source = Some(meta_source);
        self
    }

    pub async fn list_tokens(&self, query: &DexTokenListQuery) -> Result<DexTokenListResponse> {
        match query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => self.list_default(query).await,
            Some(q) => self.search(query, q).await,
        }
    }

    // -----------------------------------------------------------------------
    // Pre-fetch: RPC balances for enabled whitelist tokens (P2 ordering fix)
    // -----------------------------------------------------------------------

    /// When `account` is present, fetches on-chain balances for all enabled whitelist tokens
    /// via RPC, then resolves USD values from the `price_usd` table. Returns two aligned vecs:
    ///   - `held_ids`: token_ids with balance > 0
    ///   - `held_usd`: corresponding balance_usd (0 if price unavailable)
    ///
    /// These are injected into the SQL as `wl_rpc` CTE (UNNEST arrays) so the DB-side
    /// tier/ordering uses real on-chain data before pagination.
    async fn prefetch_wl_rpc(
        &self,
        account: &str,
    ) -> Result<(Vec<String>, Vec<BigDecimal>)> {
        // 1. Query all enabled whitelist tokens with their decimals.
        let wl_rows: Vec<(String, Option<i32>)> = sqlx::query_as::<_, (String, Option<i32>)>(
            "SELECT token_id, decimals FROM whitelist_token WHERE enabled",
        )
        .fetch_all(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query whitelist tokens: {}", e))?;

        if wl_rows.is_empty() {
            return Ok((vec![], vec![]));
        }

        // 2. Fetch RPC balance for each; keep only held (> 0).
        let mut held: Vec<(String, BigDecimal, i32)> = vec![];
        for (token_id, decimals) in &wl_rows {
            if let Some(bal) = self.balance_source.balance_of(token_id, account).await {
                if bal > BigDecimal::from(0) {
                    held.push((token_id.clone(), bal, decimals.unwrap_or(18)));
                }
            }
        }

        if held.is_empty() {
            return Ok((vec![], vec![]));
        }

        // 3. Latest USD unit price per held token from the price_usd table (DefiLlama-fed).
        let held_ids_q: Vec<String> = held.iter().map(|(id, _, _)| id.clone()).collect();
        let price_rows: Vec<(String, BigDecimal)> = sqlx::query_as::<_, (String, BigDecimal)>(
            r#"SELECT DISTINCT ON (token_id) token_id, price
               FROM price_usd
               WHERE token_id = ANY($1)
               ORDER BY token_id, block_number DESC"#,
        )
        .bind(&held_ids_q)
        .fetch_all(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query price_usd: {}", e))?;
        let prices: HashMap<String, BigDecimal> = price_rows.into_iter().collect();

        // 4. Build aligned output vecs.
        let mut held_ids: Vec<String> = Vec::with_capacity(held.len());
        let mut held_usd: Vec<BigDecimal> = Vec::with_capacity(held.len());
        for (token_id, bal, decimals) in held {
            let usd = prices
                .get(&token_id)
                .map(|p| compute_balance_usd(&bal, decimals, p))
                .unwrap_or_else(|| BigDecimal::from(0));
            held_ids.push(token_id);
            held_usd.push(usd);
        }

        Ok((held_ids, held_usd))
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
                           -- OR split into two EXISTS so idx_pool_token0/1 are usable.
                           -- A single EXISTS(token0=? OR token1=?) can't use either index →
                           -- correlated seq scan of pool per token (O(tokens × pools)).
                           AND (EXISTS (SELECT 1 FROM pool p WHERE p.token0=t.token_id)
                                OR EXISTS (SELECT 1 FROM pool p WHERE p.token1=t.token_id))
                           AND t.token_id NOT IN (SELECT token_id FROM whitelist_token WHERE enabled))
                "#,
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to count tokens: {}", e))?;

        // P2 fix: pre-fetch held whitelist balances via RPC before SQL sort.
        let (wl_ids, wl_usd) = match query.account.as_deref() {
            Some(acct) => self.prefetch_wl_rpc(acct).await?,
            None => (vec![], vec![]),
        };

        // $1=account, $2=wl_ids[], $3=wl_usd[], $4=limit, $5=offset
        let list_sql = format!(
            "{}{ENRICHED_COLS}{ORDER_CLAUSE}LIMIT $4 OFFSET $5",
            enriched_cte_sql()
        );
        let rows = measure_postgres!(
            "dex.list_tokens",
            sqlx::query_as::<_, TokenRow>(&list_sql)
                .bind(query.account.as_deref())
                .bind(&wl_ids)
                .bind(&wl_usd)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to list tokens: {}", e))?;

        let tokens = self.enrich(rows, query.account.as_deref()).await;
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
                    &search_enriched_cte_sql(TEXT_MATCHED_UNION),
                    limit,
                    offset,
                )
                .await
            }
            SearchKind::PartialCa(s) => {
                self.run_filtered_search(
                    query.account.as_deref(),
                    &format!("{}%", s),
                    &search_enriched_cte_sql(CA_MATCHED_UNION),
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
        cte: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DexTokenListResponse> {
        // P2 fix: pre-fetch held whitelist balances via RPC before SQL sort.
        let (wl_ids, wl_usd) = match account {
            Some(acct) => self.prefetch_wl_rpc(acct).await?,
            None => (vec![], vec![]),
        };

        // The $4 prefix filter lives inside `cte`'s candidate set (matched union),
        // so the outer projection needs no extra WHERE.
        // $1=account, $2=wl_ids[], $3=wl_usd[], $4=pattern, $5=limit, $6=offset
        let list_sql = format!("{cte}{ENRICHED_COLS}\n{ORDER_CLAUSE}LIMIT $5 OFFSET $6");
        // count: $1=account, $2=wl_ids[], $3=wl_usd[], $4=pattern
        let count_sql = format!("{cte}SELECT COUNT(*) FROM enriched");

        let total_count: i64 = measure_postgres!(
            "dex.search_tokens.count",
            sqlx::query_scalar::<_, i64>(&count_sql)
                .bind(account)
                .bind(&wl_ids)
                .bind(&wl_usd)
                .bind(pattern)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to count search: {}", e))?;

        let rows = measure_postgres!(
            "dex.search_tokens",
            sqlx::query_as::<_, TokenRow>(&list_sql)
                .bind(account)
                .bind(&wl_ids)
                .bind(&wl_usd)
                .bind(pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to search tokens: {}", e))?;

        let tokens = self.enrich(rows, account).await;
        Ok(DexTokenListResponse { tokens, total_count })
    }

    async fn search_full_ca(
        &self,
        query: &DexTokenListQuery,
        ca: &str,
    ) -> Result<DexTokenListResponse> {
        // P2 fix: pre-fetch for correct tier in full-CA result.
        let (wl_ids, wl_usd) = match query.account.as_deref() {
            Some(acct) => self.prefetch_wl_rpc(acct).await?,
            None => (vec![], vec![]),
        };

        // $1=account, $2=wl_ids[], $3=wl_usd[], $4=ca
        let sql = search_full_ca_sql();
        let rows = measure_postgres!(
            "dex.search_full_ca",
            sqlx::query_as::<_, TokenRow>(&sql)
                .bind(query.account.as_deref())
                .bind(&wl_ids)
                .bind(&wl_usd)
                .bind(ca)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed full CA search: {}", e))?;

        // Unindexed external token: present in no table → resolve on-chain
        // metadata via RPC and surface it as an external entry (no price/balance,
        // it isn't in any pool we track). No meta source / no on-chain match →
        // fall through to the empty result below.
        if rows.is_empty() {
            if let Some(entry) = self.fetch_external_meta(ca).await {
                return Ok(DexTokenListResponse { tokens: vec![entry], total_count: 1 });
            }
        }

        let total_count = rows.len() as i64;
        let tokens = self.enrich(rows, query.account.as_deref()).await;
        Ok(DexTokenListResponse { tokens, total_count })
    }

    /// On-chain ERC20 metadata fallback for a full CA absent from every table.
    /// Returns an `external` entry with RPC-sourced name/symbol/decimals; price
    /// and balance stay null (the token is in no pool/market we index).
    async fn fetch_external_meta(&self, ca: &str) -> Option<DexTokenEntry> {
        let meta = self.meta_source.as_ref()?.token_meta(ca).await?;
        Some(DexTokenEntry {
            token_id: ca.to_string(),
            symbol: meta.symbol,
            name: meta.name,
            decimals: meta.decimals,
            image_uri: String::new(),
            token_type: "external".to_string(),
            balance: None,
            balance_usd: None,
            price_usd: None,
        })
    }

    /// whitelist + account 행만 RPC 잔액 + Pyth 가격으로 보강. 나머지는 row_to_entry 그대로.
    /// NOTE: enrich() still formats balance/balance_usd strings for whitelist rows.
    /// The tier/ordering is already correct from SQL (wl_rpc CTE). Double RPC calls are
    /// acceptable here (curated set is small); a cache layer can be added later if needed.
    async fn enrich(&self, rows: Vec<TokenRow>, account: Option<&str>) -> Vec<DexTokenEntry> {
        // price_usd comes solely from the price_usd table (DefiLlama-fed) via SQL — no
        // external price source. Whitelist tokens with no price_usd row → price None.
        // balance / balance_usd require an account: fetch held whitelist balances via RPC.
        let mut wl_balance: HashMap<String, BigDecimal> = HashMap::new();
        if let Some(account) = account {
            for r in rows.iter().filter(|r| r.token_type == "whitelist") {
                if let Some(b) = self.balance_source.balance_of(&r.token_id, account).await {
                    if b > BigDecimal::from(0) {
                        wl_balance.insert(r.token_id.clone(), b);
                    }
                }
            }
        }

        rows.into_iter()
            .map(|r| {
                if r.token_type == "whitelist" {
                    let price_usd = r.price_usd.clone();
                    let balance = wl_balance.get(&r.token_id).cloned();
                    let decimals = r.decimals.unwrap_or(18);
                    let balance_usd = match (&balance, &price_usd) {
                        (Some(b), Some(p)) => Some(compute_balance_usd(b, decimals, p)),
                        _ => None,
                    };
                    whitelist_entry(r, balance, balance_usd, price_usd)
                } else {
                    row_to_entry(r)
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// SQL builders — shared by default list and search paths
// ---------------------------------------------------------------------------

/// Returns the ENRICHED_CTE with wl_rpc injected for P2 ordering fix.
///
/// Bind indices:
///   $1 = account (nullable VARCHAR)
///   $2 = held whitelist token_ids (VARCHAR[])  — may be empty
///   $3 = held whitelist balance_usd  (NUMERIC[]) — may be empty
///
/// The wl_rpc CTE holds RPC-derived (token_id, balance_usd) for held whitelist tokens.
/// It is LEFT JOINed into `enriched`, overriding the DB balance table for tier/order.
/// Default-list candidate CTEs: whitelist ∪ (nadfun V2 with a pool). Slots into
/// `enriched_cte_with` between wl_rpc and enriched; must end with `candidates AS (…),`.
const DEFAULT_CANDIDATES_CTES: &str = r#"
wl AS (
    SELECT token_id, sort_order, name, symbol, image_uri, decimals FROM whitelist_token WHERE enabled
),
v2 AS (
    SELECT t.token_id
    FROM token t
    WHERE t.version='V2'
      -- OR split into two EXISTS so idx_pool_token0/1 are usable. A single
      -- EXISTS(token0=? OR token1=?) can't use either index → correlated seq
      -- scan of pool per token (O(tokens × pools); ~157s at 50k V2 tokens).
      AND (EXISTS (SELECT 1 FROM pool p WHERE p.token0=t.token_id)
           OR EXISTS (SELECT 1 FROM pool p WHERE p.token1=t.token_id))
      AND t.token_id NOT IN (SELECT token_id FROM wl)
),
candidates AS (
    SELECT token_id, 'whitelist'::text AS token_type, sort_order, name, symbol, image_uri, decimals FROM wl
    UNION ALL
    SELECT token_id, 'nadfun_v2'::text AS token_type, NULL::int AS sort_order,
           NULL::varchar AS name, NULL::varchar AS symbol, NULL::varchar AS image_uri, NULL::int AS decimals FROM v2
),
"#;

/// name/symbol search universe: token ∪ dex_token ∪ quote_token ∪ whitelist,
/// prefix-matched (`$4`). Feeds `search_enriched_cte_sql`.
const TEXT_MATCHED_UNION: &str = r#"
    SELECT token_id FROM token            WHERE name ILIKE $4 OR symbol ILIKE $4
    UNION SELECT token_id FROM dex_token   WHERE name ILIKE $4 OR symbol ILIKE $4
    UNION SELECT quote_id FROM quote_token WHERE name ILIKE $4 OR symbol ILIKE $4
    UNION SELECT token_id FROM whitelist_token WHERE enabled AND (name ILIKE $4 OR symbol ILIKE $4)
"#;

/// CA (partial/prefix) search universe — same tables, matched on token_id (`$4`).
const CA_MATCHED_UNION: &str = r#"
    SELECT token_id FROM token            WHERE token_id ILIKE $4
    UNION SELECT token_id FROM dex_token   WHERE token_id ILIKE $4
    UNION SELECT quote_id FROM quote_token WHERE quote_id ILIKE $4
    UNION SELECT token_id FROM whitelist_token WHERE enabled AND token_id ILIKE $4
"#;

/// Default list candidate set (whitelist + nadfun V2 with pool).
fn enriched_cte_sql() -> String {
    enriched_cte_with(DEFAULT_CANDIDATES_CTES)
}

/// Search candidate set spanning token ∪ dex_token ∪ quote_token ∪ whitelist.
/// token_type is by table membership: whitelist_token → whitelist, `token`
/// version V2/V1 → nadfun_v2/nadfun_v1, otherwise → external. `matched_union`
/// supplies the prefix filter (name/symbol or token_id) over `$4`.
fn search_enriched_cte_sql(matched_union: &str) -> String {
    let mid = format!(
        r#"
matched AS (
{matched_union}
),
candidates AS (
    SELECT m.token_id,
           CASE WHEN wl.token_id IS NOT NULL THEN 'whitelist'
                WHEN t.version = 'V2'        THEN 'nadfun_v2'
                WHEN t.version = 'V1'        THEN 'nadfun_v1'
                ELSE 'external' END::text AS token_type,
           wl.sort_order, wl.name, wl.symbol, wl.image_uri, wl.decimals
    FROM matched m
    LEFT JOIN whitelist_token wl ON wl.token_id = m.token_id AND wl.enabled
    LEFT JOIN token           t  ON t.token_id  = m.token_id
),
"#
    );
    enriched_cte_with(&mid)
}

/// Shared enrichment template. `mid_ctes` defines `candidates` (plus any helper
/// CTEs) and slots between wl_rpc and enriched; it must end with a trailing comma.
fn enriched_cte_with(mid_ctes: &str) -> String {
    format!(
        r#"
WITH wl_rpc AS (
    SELECT t.token_id, t.balance_usd
    FROM UNNEST($2::varchar[], $3::numeric[]) AS t(token_id, balance_usd)
),
{mid_ctes}
enriched AS (
    SELECT
        c.token_id,
        c.token_type,
        c.sort_order,
        COALESCE(c.symbol,    t.symbol,    dt.symbol,    qt.symbol)    AS symbol,
        COALESCE(c.name,      t.name,      dt.name,      qt.name)      AS name,
        COALESCE(c.decimals,  dt.decimals, qt.decimals)   AS decimals,
        COALESCE(c.image_uri, t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
        b.balance AS balance,
        COALESCE(
            wl_rpc.balance_usd,
            (b.balance / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC
                * COALESCE(dtp.price_usd, m.price * lp.price, pu.price))
        ) AS balance_usd,
        (m.price * (t.total_supply / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC)
            * lp.price) AS market_cap_usd,
        -- per-token USD unit price. Pool view (deepest-TVL pool's per-token USD)
        -- first, legacy market.price × quote→USD next, then the price_usd table
        -- (DefiLlama; covers whitelist which has no market/pool row).
        -- account-independent.
        COALESCE(dtp.price_usd, m.price * lp.price, pu.price) AS price_usd,
        CASE
          WHEN $1 IS NOT NULL AND wl_rpc.token_id IS NOT NULL AND c.token_type='whitelist' THEN 1
          -- held (balance > 0) non-whitelist floats up: nadfun_v2/v1 and external
          -- alike (search surfaces V1/external; holding any of them ranks above
          -- unheld). Default list only has nadfun_v2 candidates here, so unchanged.
          WHEN $1 IS NOT NULL AND b.balance > 0 AND c.token_type <> 'whitelist' THEN 2
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
    LEFT JOIN wl_rpc      ON wl_rpc.token_id = c.token_id
    LEFT JOIN dex_token_price dtp ON dtp.token_id = c.token_id
    LEFT JOIN LATERAL (
        SELECT price FROM price_usd WHERE token_id = c.token_id
        ORDER BY block_number DESC LIMIT 1
    ) pu ON true
)
"#
    )
}

const ENRICHED_COLS: &str = r#"SELECT token_id, token_type, symbol, name, decimals, image_uri,
       balance, balance_usd, price_usd
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
/// $1 = account (nullable), $2 = wl_ids[], $3 = wl_usd[], $4 = exact token_id (EIP-55 checksum).
fn search_full_ca_sql() -> String {
    format!(
        r#"
WITH wl_rpc AS (
    SELECT t.token_id, t.balance_usd
    FROM UNNEST($2::varchar[], $3::numeric[]) AS t(token_id, balance_usd)
),
target AS (SELECT $4::VARCHAR AS token_id)
SELECT
    tg.token_id,
    CASE
      WHEN wl.token_id IS NOT NULL THEN 'whitelist'
      WHEN t.version = 'V2' THEN 'nadfun_v2'
      WHEN t.version = 'V1' THEN 'nadfun_v1'
      ELSE 'external'
    END AS token_type,
    COALESCE(wl.symbol,    t.symbol,    dt.symbol,    qt.symbol)    AS symbol,
    COALESCE(wl.name,      t.name,      dt.name,      qt.name)      AS name,
    COALESCE(wl.decimals, dt.decimals, qt.decimals)   AS decimals,
    COALESCE(wl.image_uri, t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
    b.balance AS balance,
    COALESCE(
        wl_rpc.balance_usd,
        (b.balance / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::NUMERIC * COALESCE(dtp.price_usd, m.price * lp.price, pu.price))
    ) AS balance_usd,
    COALESCE(dtp.price_usd, m.price * lp.price, pu.price) AS price_usd
FROM target tg
LEFT JOIN whitelist_token wl ON wl.token_id = tg.token_id AND wl.enabled
LEFT JOIN wl_rpc              ON wl_rpc.token_id = tg.token_id
LEFT JOIN token       t  ON t.token_id  = tg.token_id
LEFT JOIN dex_token   dt ON dt.token_id = tg.token_id
LEFT JOIN quote_token qt ON qt.quote_id = tg.token_id
LEFT JOIN balance     b  ON b.token_id  = tg.token_id AND b.account_id = COALESCE($1::VARCHAR, '__NO_ACCOUNT__')
LEFT JOIN market      m  ON m.token_id  = tg.token_id
LEFT JOIN LATERAL (SELECT price FROM price WHERE quote_id=m.quote_id ORDER BY block_number DESC LIMIT 1) lp ON true
LEFT JOIN dex_token_price dtp ON dtp.token_id = tg.token_id
LEFT JOIN LATERAL (SELECT price FROM price_usd WHERE token_id = tg.token_id ORDER BY block_number DESC LIMIT 1) pu ON true
WHERE (t.token_id IS NOT NULL OR dt.token_id IS NOT NULL OR qt.quote_id IS NOT NULL OR wl.token_id IS NOT NULL)
"#
    )
}

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

fn row_to_entry(r: TokenRow) -> DexTokenEntry {
    // 보유(balance > 0)일 때만 balance/balance_usd 노출. 미보유·계정 미제공이면 둘 다 null →
    // FE는 balance != null로 보유 판정. balance_usd는 balance에 결합한다(codex P2: 0-잔고 행에서
    // balance=null인데 balance_usd="0"으로 어긋나는 것 방지).
    let held = r.balance.filter(|b| *b > BigDecimal::from(0));
    let balance_usd = if held.is_some() { r.balance_usd } else { None };
    DexTokenEntry {
        token_id: r.token_id,
        symbol: r.symbol.unwrap_or_default(),
        name: r.name.unwrap_or_default(),
        decimals: r.decimals.unwrap_or(18),
        image_uri: r.image_uri.unwrap_or_default(),
        token_type: r.token_type,
        balance: held.map(|b| b.normalized().to_plain_string()),
        balance_usd: balance_usd.map(|v| v.normalized().to_plain_string()),
        price_usd: r.price_usd.map(|v| fmt_price_usd(&v)),
    }
}

/// Format a USD unit price: truncate to 8 decimal places (round toward zero),
/// then strip trailing zeros. Keeps responses to ≤8 decimals.
fn fmt_price_usd(v: &BigDecimal) -> String {
    v.with_scale_round(8, bigdecimal::RoundingMode::Down)
        .normalized()
        .to_plain_string()
}

fn whitelist_entry(
    r: TokenRow,
    balance: Option<BigDecimal>,
    balance_usd: Option<BigDecimal>,
    price_usd: Option<BigDecimal>,
) -> DexTokenEntry {
    DexTokenEntry {
        token_id: r.token_id,
        symbol: r.symbol.unwrap_or_default(),
        name: r.name.unwrap_or_default(),
        decimals: r.decimals.unwrap_or(18),
        image_uri: r.image_uri.unwrap_or_default(),
        token_type: r.token_type,
        balance: balance.as_ref().map(|b| b.normalized().to_plain_string()),
        balance_usd: balance_usd.map(|v| v.normalized().to_plain_string()),
        price_usd: price_usd.map(|v| fmt_price_usd(&v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    use crate::services::pricing::{BalanceSource, PriceSource, TokenMeta, TokenMetaSource};
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ACCOUNT: &str = "0x000000000000000000000000000000000000aA11";
    const POOL: &str = "0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01"; // CHOG, launchpad
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02"; // WMON, dex_token only

    const WL_A: &str = "0x000000000000000000000000000000000000cC01"; // whitelist sort 1
    const WL_B: &str = "0x000000000000000000000000000000000000cC02"; // whitelist sort 2

    // -----------------------------------------------------------------------
    // Fake sources
    // -----------------------------------------------------------------------

    struct FakeBalance {
        balances: HashMap<(String, String), BigDecimal>, // (token, account) → wei
        calls: AtomicUsize,
    }

    impl Default for FakeBalance {
        fn default() -> Self {
            Self { balances: HashMap::new(), calls: AtomicUsize::new(0) }
        }
    }

    #[async_trait::async_trait]
    impl BalanceSource for FakeBalance {
        async fn balance_of(&self, token_id: &str, account: &str) -> Option<BigDecimal> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.balances.get(&(token_id.to_string(), account.to_string())).cloned()
        }
    }

    #[derive(Default)]
    struct FakeMeta {
        metas: HashMap<String, TokenMeta>, // token_id → on-chain meta
    }

    #[async_trait::async_trait]
    impl TokenMetaSource for FakeMeta {
        async fn token_meta(&self, token_id: &str) -> Option<TokenMeta> {
            self.metas.get(token_id).cloned()
        }
    }

    // -----------------------------------------------------------------------
    // Controller constructors
    // -----------------------------------------------------------------------

    fn make_controller(pool: PgPool) -> TokensController {
        use crate::db::postgres::PostgresDatabase;
        TokensController::with_sources(
            std::sync::Arc::new(PostgresDatabase { write_pool: pool.clone(), read_pool: pool }),
            std::sync::Arc::new(FakeBalance::default()),
        )
    }

    fn controller_with(pool: PgPool, bal: FakeBalance) -> TokensController {
        use crate::db::postgres::PostgresDatabase;
        TokensController::with_sources(
            std::sync::Arc::new(PostgresDatabase { write_pool: pool.clone(), read_pool: pool }),
            std::sync::Arc::new(bal),
        )
    }

    fn empty_query() -> DexTokenListQuery {
        DexTokenListQuery { account: None, q: None, page: 1, limit: 50 }
    }

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

    async fn seed_whitelist_with_feed(pool: &PgPool, token_id: &str, order: i32, feed: &str) {
        sqlx::query("INSERT INTO whitelist_token (token_id, sort_order, price_feed_id) VALUES ($1,$2,$3)")
            .bind(token_id).bind(order).bind(feed).execute(pool).await.unwrap();
        // 메타(symbol/name/decimals)는 dex_token에서 옴
        sqlx::query(r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
                       VALUES ($1,'USD Coin','USDC',18,'',0)"#)
            .bind(token_id).execute(pool).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Existing tests
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
        assert_eq!(chog.token_type, "nadfun_v2", "nadfun V2 (external 아님)");

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
        assert_eq!(resp.tokens[0].token_type, "whitelist");
        assert_eq!(resp.tokens[1].token_id, WL_B);
        assert_eq!(resp.tokens[2].token_id, TOKEN0); // CHOG nadfun V2 (whitelist 다음)
        assert_eq!(resp.tokens[2].token_type, "nadfun_v2");
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
        // 보유 nadfun V2(CHOG)가 미보유 화이트리스트(WL_A)보다 상위로 정렬돼야 한다.
        let chog_idx = resp.tokens.iter().position(|t| t.token_id == TOKEN0).expect("CHOG present");
        let wl_idx = resp.tokens.iter().position(|t| t.token_id == WL_A).expect("WL_A present");
        assert!(chog_idx < wl_idx, "보유 nadfun V2가 미보유 화이트리스트보다 상위");
        let chog = &resp.tokens[chog_idx];
        assert_eq!(chog.balance.as_deref(), Some("1000000000000000000000"), "보유 → balance 노출");
        assert!(resp.tokens[wl_idx].balance.is_none(), "미보유 → balance null (account 제공돼도)");
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
            .filter(|t| t.token_type == "whitelist").map(|t| t.token_id.as_str()).collect();
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
    }

    /// Policy change: CA search (incl. partial) now covers dex_token, so indexed
    /// external tokens surface by CA prefix too (was: partial-CA excluded external).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_partial_ca_includes_external(pool: PgPool) {
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
        let ext = resp
            .tokens
            .iter()
            .find(|t| t.token_id == TOKEN1)
            .expect("external present by partial CA");
        assert_eq!(ext.token_type, "external", "partial CA는 dex_token(external)도 노출");
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

    /// Policy change: name/symbol search now covers dex_token, so an external
    /// token whose symbol prefix-matches surfaces (was: text search excluded external).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_text_includes_external_symbol_match(pool: PgPool) {
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
        let ext = resp
            .tokens
            .iter()
            .find(|t| t.token_id == TOKEN1)
            .expect("external present by symbol prefix");
        assert_eq!(ext.token_type, "external", "text 검색은 dex_token(external) symbol prefix도 노출");
    }

    /// V1 nadfun token (in `token`, version V1) must surface in name/symbol search,
    /// labeled nadfun_v1. Previously invisible — search reused the V2-only candidate set.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_text_finds_v1(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await; // TOKEN0 = CHOG, version stays V1
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
        let v1 = resp
            .tokens
            .iter()
            .find(|t| t.token_id == TOKEN0)
            .expect("V1 token present by symbol prefix");
        assert_eq!(v1.token_type, "nadfun_v1", "V1 nadfun token → nadfun_v1");
    }

    /// Search: a held token (balance > 0) must sort above unheld results even when
    /// it's nadfun_v1/external — previously only nadfun_v2 got the held tier, so a
    /// held V1 sank to tier 4 below unheld tokens.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_held_v1_sorts_above_unheld(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await; // TOKEN0 = V1 "CHOG" (token row)
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // hold CHOG
        // unheld external that also prefix-matches "CHO"
        const OTHER: &str = "0x000000000000000000000000000000000000c0DE";
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'Choco', 'CHOC', 18, '', 0)"#,
        )
        .bind(OTHER)
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);

        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: Some(ACCOUNT.to_string()),
                q: Some("CHO".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        let held = resp
            .tokens
            .iter()
            .position(|t| t.token_id == TOKEN0)
            .expect("held V1 present");
        let unheld = resp
            .tokens
            .iter()
            .position(|t| t.token_id == OTHER)
            .expect("unheld external present");
        assert!(
            held < unheld,
            "held (balance>0) sorts above unheld in search (held={held}, unheld={unheld})"
        );
    }

    /// V1 nadfun token found by partial CA too.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_partial_ca_finds_v1(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let prefix = &TOKEN0[..10];
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(prefix.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        let v1 = resp
            .tokens
            .iter()
            .find(|t| t.token_id == TOKEN0)
            .expect("V1 present by partial CA");
        assert_eq!(v1.token_type, "nadfun_v1");
    }

    /// full-CA on a V1 token labels it nadfun_v1 (was external under the version='V2' CASE).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_v1_labeled_nadfun_v1(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await; // TOKEN0 V1
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(TOKEN0.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        let v1 = resp
            .tokens
            .iter()
            .find(|t| t.token_id == TOKEN0)
            .expect("V1 present by full CA");
        assert_eq!(v1.token_type, "nadfun_v1", "full-CA V1 → nadfun_v1 (not external)");
    }

    // -----------------------------------------------------------------------
    // row_to_entry 불변식 (codex P2): balance == null ⇒ balance_usd == null
    // -----------------------------------------------------------------------

    fn token_row(balance: Option<i64>, balance_usd: Option<i64>) -> TokenRow {
        TokenRow {
            token_id: "0x000000000000000000000000000000000000dEaD".into(),
            token_type: "nadfun_v2".into(),
            symbol: Some("X".into()),
            name: Some("X".into()),
            decimals: Some(18),
            image_uri: Some(String::new()),
            balance: balance.map(BigDecimal::from),
            balance_usd: balance_usd.map(BigDecimal::from),
            price_usd: None,
        }
    }

    #[test]
    fn balance_usd_null_when_not_held() {
        // 0-잔고 행: balance는 null로 떨어지는데 balance_usd가 "0"으로 남으면 불일치.
        let e = row_to_entry(token_row(Some(0), Some(0)));
        assert!(e.balance.is_none(), "0 잔고 → balance null");
        assert!(e.balance_usd.is_none(), "balance null이면 balance_usd도 null");
    }

    #[test]
    fn balance_and_balance_usd_present_when_held() {
        let e = row_to_entry(token_row(Some(1000), Some(42)));
        assert_eq!(e.balance.as_deref(), Some("1000"));
        assert_eq!(e.balance_usd.as_deref(), Some("42"));
    }

    // -----------------------------------------------------------------------
    // Task 5 (whitelist enrichment) — new tests
    // -----------------------------------------------------------------------

    const WL_FEED: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[sqlx::test(migrations = "./migrations-test")]
    async fn whitelist_balance_from_rpc_and_usd_from_price_usd_table(pool: PgPool) {
        seed_whitelist_with_feed(&pool, WL_A, 1, WL_FEED).await;
        // latest price_usd row → $1.5 (no external price source)
        sqlx::query("INSERT INTO price_usd (token_id, block_number, price, created_at) VALUES ($1, 1, 1.5, 0)")
            .bind(WL_A).execute(&pool).await.unwrap();
        let mut bal = FakeBalance::default();
        bal.balances.insert(
            (WL_A.to_string(), ACCOUNT.to_string()),
            BigDecimal::from_str("2000000000000000000").unwrap(), // 2 tokens (18dp)
        );
        let c = controller_with(pool, bal);
        let resp = c.list_tokens(&DexTokenListQuery {
            account: Some(ACCOUNT.to_string()), q: None, page: 1, limit: 50,
        }).await.unwrap();
        let t = resp.tokens.iter().find(|t| t.token_id == WL_A).unwrap();
        assert_eq!(t.balance.as_deref(), Some("2000000000000000000"), "RPC 잔액 노출");
        // normalized() strips trailing zeros: 3.0 → "3". Compare as BigDecimal for canonical equality.
        let usd: BigDecimal = t.balance_usd.as_deref().unwrap().parse().unwrap();
        assert_eq!(usd, BigDecimal::from_str("3.0").unwrap(), "2 × $1.5 = $3 (price from price_usd table)");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn whitelist_unheld_has_null_balance_and_no_price_call(pool: PgPool) {
        seed_whitelist_with_feed(&pool, WL_A, 1, WL_FEED).await;
        let bal = FakeBalance::default(); // 잔액 없음 → 미보유
        let c = controller_with(pool, bal);
        let resp = c.list_tokens(&DexTokenListQuery {
            account: Some(ACCOUNT.to_string()), q: None, page: 1, limit: 50,
        }).await.unwrap();
        let t = resp.tokens.iter().find(|t| t.token_id == WL_A).unwrap();
        assert!(t.balance.is_none(), "미보유 → balance null");
        assert!(t.balance_usd.is_none(), "미보유 → balance_usd null");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn no_account_skips_balance_source(pool: PgPool) {
        seed_whitelist_with_feed(&pool, WL_A, 1, WL_FEED).await;
        let bal = std::sync::Arc::new(FakeBalance::default());
        let c = {
            use crate::db::postgres::PostgresDatabase;
            TokensController::with_sources(
                std::sync::Arc::new(PostgresDatabase { write_pool: pool.clone(), read_pool: pool }),
                bal.clone(),
            )
        };
        let _ = c.list_tokens(&empty_query()).await.unwrap();
        assert_eq!(bal.calls.load(Ordering::SeqCst), 0, "account 없으면 balanceOf 호출 0");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn whitelist_uses_own_metadata_over_join_tables(pool: PgPool) {
        // whitelist_token에 직접 넣은 name/symbol/image_uri는 token/dex_token/quote_token
        // 행이 전혀 없어도 그대로 노출돼야 한다 (whitelist는 self-described 우선).
        sqlx::query(
            "INSERT INTO whitelist_token (token_id, sort_order, name, symbol, image_uri, decimals)
             VALUES ($1, 1, 'USD Coin', 'USDC', 'https://img/usdc.png', 6)",
        )
        .bind(WL_A)
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        let t = resp.tokens.iter().find(|t| t.token_id == WL_A).expect("WL_A present");
        assert_eq!(t.symbol, "USDC", "whitelist_token.symbol 우선");
        assert_eq!(t.name, "USD Coin", "whitelist_token.name 우선");
        assert_eq!(t.image_uri, "https://img/usdc.png", "whitelist_token.image_uri 우선");
        assert_eq!(t.decimals, 6, "whitelist_token.decimals 우선 (join 없어도)");
    }

    // -----------------------------------------------------------------------
    // Codex P2 fix — whitelist-only token must appear in full-CA search
    // -----------------------------------------------------------------------

    /// Token present ONLY in whitelist_token (not in token/dex_token/quote_token).
    /// Full-CA search must return it with token_type="whitelist".
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_returns_whitelist_only_token(pool: PgPool) {
        // WL_A exists only in whitelist_token — no row in token/dex_token/quote_token.
        sqlx::query(
            "INSERT INTO whitelist_token (token_id, sort_order, name, symbol, image_uri, decimals)
             VALUES ($1, 1, 'USD Coin', 'USDC', 'https://img/usdc.png', 6)",
        )
        .bind(WL_A)
        .execute(&pool)
        .await
        .unwrap();

        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(WL_A.to_string()), // exact full-CA search
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();

        assert_eq!(resp.tokens.len(), 1, "whitelist-only token이 full-CA 검색에 나와야 함");
        assert_eq!(resp.tokens[0].token_id, WL_A, "token_id 일치");
        assert_eq!(resp.tokens[0].token_type, "whitelist", "token_type은 whitelist");
        assert_eq!(resp.tokens[0].symbol, "USDC", "whitelist 메타데이터 그대로 노출");
    }

    // -----------------------------------------------------------------------
    // P2 ordering fix — held whitelist (off-DEX, RPC balance) sorts before held V2
    // -----------------------------------------------------------------------

    /// Proves the P2 ordering bug is fixed:
    /// A whitelist token with NO DB balance row (off-DEX) but held on-chain (RPC)
    /// must sort as tier=1 and appear BEFORE a held nadfun V2 token (tier=2).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn held_whitelist_rpc_sorts_before_held_v2(pool: PgPool) {
        // Seed: CHOG as nadfun V2, account holds it in DB balance (tier 2).
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0)
            .execute(&pool)
            .await
            .unwrap();
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // CHOG held → tier2

        // WL_A: off-DEX whitelist token — NO DB balance row, but RPC returns held balance.
        seed_whitelist_with_feed(&pool, WL_A, 1, WL_FEED).await;
        // (no seed_balance for WL_A — off-DEX, not in DB balance table)

        // Mock: RPC reports WL_A held for ACCOUNT.
        let mut bal = FakeBalance::default();
        bal.balances.insert(
            (WL_A.to_string(), ACCOUNT.to_string()),
            BigDecimal::from_str("5000000000000000000").unwrap(), // 5 tokens (18dp)
        );
        let c = controller_with(pool, bal);
        let resp = c.list_tokens(&DexTokenListQuery {
            account: Some(ACCOUNT.to_string()),
            q: None,
            page: 1,
            limit: 50,
        }).await.unwrap();

        let wl_idx = resp.tokens.iter().position(|t| t.token_id == WL_A)
            .expect("WL_A (held whitelist) must be present");
        let v2_idx = resp.tokens.iter().position(|t| t.token_id == TOKEN0)
            .expect("CHOG (held V2) must be present");

        // Core assertion: held whitelist (tier 1) before held V2 (tier 2).
        assert!(
            wl_idx < v2_idx,
            "held whitelist (RPC, off-DEX) must sort before held nadfun V2 (positions: WL_A={wl_idx}, CHOG={v2_idx})"
        );

        // Confirm WL_A exposes balance (set by enrich()).
        let wl_tok = &resp.tokens[wl_idx];
        assert!(wl_tok.balance.is_some(), "held whitelist must have balance set");
    }

    // -----------------------------------------------------------------------
    // price_usd (per-token USD unit price, account-independent, 8dp truncated)
    // -----------------------------------------------------------------------

    async fn seed_market_price(pool: &PgPool, token_id: &str, market_price: &str, quote_usd: &str) {
        // market.quote_id and price.quote_id MUST match exactly (case-sensitive
        // join), so pin both to the same address rather than relying on defaults.
        const MON_QUOTE: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A";
        sqlx::query(
            r#"INSERT INTO market (market_type, token_id, price, quote_id, latest_trade_at, created_at)
               VALUES ('V2_DEX', $1, $2::NUMERIC, $3, 0, 0)"#,
        )
        .bind(token_id).bind(market_price).bind(MON_QUOTE)
        .execute(pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO price (quote_id, block_number, price) VALUES ($1, 1, $2::NUMERIC)"#,
        )
        .bind(MON_QUOTE).bind(quote_usd)
        .execute(pool).await.unwrap();
    }

    /// whitelist price_usd comes from the price_usd table and must be present even with
    /// NO account (price is account-independent), truncated to 8 decimals.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn price_usd_for_whitelist_without_account(pool: PgPool) {
        const FEED: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        seed_whitelist_with_feed(&pool, WL_A, 1, FEED).await;
        // price_usd table → 2.123456789 USD; 8dp truncate → "2.12345678"
        sqlx::query("INSERT INTO price_usd (token_id, block_number, price, created_at) VALUES ($1, 1, 2.123456789, 0)")
            .bind(WL_A).execute(&pool).await.unwrap();
        let controller = controller_with(pool, FakeBalance::default());

        // No ?account= — price_usd must still be populated.
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        let wl = resp.tokens.iter().find(|t| t.token_id == WL_A).expect("whitelist present");
        assert!(wl.balance.is_none(), "no account → balance None");
        assert!(wl.balance_usd.is_none(), "no account → balance_usd None");
        assert_eq!(
            wl.price_usd.as_deref(),
            Some("2.12345678"),
            "whitelist price_usd from price_usd table, account-independent, 8dp truncated"
        );
    }

    /// nadfun_v2 price_usd = market.price × quote→USD, from SQL, account-independent.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn price_usd_for_v2_from_market(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0).execute(&pool).await.unwrap();
        // m.price=3, quote→USD=0.5 → price_usd = 1.5
        seed_market_price(&pool, TOKEN0, "3", "0.5").await;
        let controller = make_controller(pool);

        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert_eq!(
            chog.price_usd.as_deref(),
            Some("1.5"),
            "nadfun_v2 price_usd = market.price × quote→USD (account-independent)"
        );
    }

    /// Set the per-token USD unit price the observer's RawSync inference writes
    /// onto a pool. `side` is 0 or 1 (which side of POOL the token sits on).
    async fn seed_pool_token_price_usd(pool: &PgPool, side: u8, price_usd: &str) {
        let col = if side == 0 { "token0_price_usd" } else { "token1_price_usd" };
        sqlx::query(&format!("UPDATE pool SET {col} = $1::NUMERIC WHERE pool_id = $2"))
            .bind(price_usd).bind(POOL)
            .execute(pool).await.unwrap();
    }

    /// pure-DEX nadfun_v2 token with NO market row: price_usd must come from the
    /// dex_token_price view (deepest-TVL pool's per-token USD), not market.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn price_usd_for_pure_dex_v2_from_pool_view(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0).execute(&pool).await.unwrap();
        // No market row. Observer set TOKEN0 (token0 side) USD price = 0.5 on the pool.
        seed_pool_token_price_usd(&pool, 0, "0.5").await;
        let controller = make_controller(pool);

        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert_eq!(
            chog.price_usd.as_deref(),
            Some("0.5"),
            "pure-dex nadfun_v2 price_usd = pool view (deepest-TVL token0_price_usd), market absent"
        );
    }

    /// Held pure-DEX token (pool-view price, no market row): balance_usd must use
    /// the same unit price as price_usd. codex P2 — was null because balance_usd
    /// only multiplied the market path (m.price × lp.price).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn balance_usd_for_held_pure_dex_from_pool_view(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0).execute(&pool).await.unwrap();
        seed_pool_token_price_usd(&pool, 0, "0.5").await; // pool view unit price, NO market row
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // 1000 CHOG (18dp)
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
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert_eq!(chog.price_usd.as_deref(), Some("0.5"), "price from pool view");
        let usd: BigDecimal = chog
            .balance_usd
            .as_deref()
            .expect("held → balance_usd present")
            .parse()
            .unwrap();
        assert_eq!(
            usd,
            BigDecimal::from_str("500").unwrap(),
            "balance_usd = 1000 × 0.5 (pool-view unit price, not market)"
        );
    }

    /// When both a market row and a pool view price exist, the pool view wins
    /// (COALESCE(view, market) — pool is the canonical single source).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn price_usd_pool_view_takes_precedence_over_market(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0).execute(&pool).await.unwrap();
        // market path would yield 3 × 0.5 = 1.5; pool view yields 0.5 → view wins.
        seed_market_price(&pool, TOKEN0, "3", "0.5").await;
        seed_pool_token_price_usd(&pool, 0, "0.5").await;
        let controller = make_controller(pool);

        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert_eq!(
            chog.price_usd.as_deref(),
            Some("0.5"),
            "pool view price_usd takes precedence over market path"
        );
    }

    /// Full-CA search path resolves price_usd from the pool view too (second SQL
    /// builder: search_full_ca_sql). Pure-dex token, no market row.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn price_usd_full_ca_search_from_pool_view(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query("UPDATE token SET version='V2' WHERE token_id=$1")
            .bind(TOKEN0).execute(&pool).await.unwrap();
        seed_pool_token_price_usd(&pool, 0, "0.25").await;
        let controller = make_controller(pool);

        // q = exact full CA → search_full_ca path.
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(TOKEN0.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert_eq!(
            chog.price_usd.as_deref(),
            Some("0.25"),
            "full-CA search price_usd = pool view (search_full_ca_sql)"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 2 — full-CA RPC metadata fallback for unindexed external tokens
    // -----------------------------------------------------------------------

    const UNKNOWN_CA: &str = "0x000000000000000000000000000000000000FeeD";

    /// full CA that is in NO table → fall back to on-chain meta and surface it
    /// as an external token (name/symbol/decimals from RPC, price/balance null).
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_rpc_fallback_external(pool: PgPool) {
        let mut meta = FakeMeta::default();
        meta.metas.insert(
            UNKNOWN_CA.to_string(),
            TokenMeta { name: "Foreign".into(), symbol: "FRGN".into(), decimals: 6 },
        );
        let controller = make_controller(pool).with_meta_source(std::sync::Arc::new(meta));

        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(UNKNOWN_CA.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();

        assert_eq!(resp.tokens.len(), 1, "RPC-resolved external surfaces");
        assert_eq!(resp.total_count, 1);
        let t = &resp.tokens[0];
        assert_eq!(t.token_id, UNKNOWN_CA);
        assert_eq!(t.token_type, "external");
        assert_eq!(t.symbol, "FRGN");
        assert_eq!(t.name, "Foreign");
        assert_eq!(t.decimals, 6);
        assert!(t.price_usd.is_none(), "unindexed external has no price");
        assert!(t.balance.is_none(), "no balance for RPC-fallback external");
    }

    /// meta source present but the address resolves to nothing on-chain → empty.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_full_ca_meta_miss_returns_empty(pool: PgPool) {
        let controller =
            make_controller(pool).with_meta_source(std::sync::Arc::new(FakeMeta::default()));
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some(UNKNOWN_CA.to_string()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(resp.tokens.is_empty(), "meta miss → no entry");
        assert_eq!(resp.total_count, 0);
    }
}
