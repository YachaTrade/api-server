use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{
                AccountInfo, FeeInfo, MarketInfo, MarketType, QuoteInfo, RewardInfo, TokenInfo,
                TokenVersion,
            },
            pagination::PaginationParams,
        },
        dex::tokens::{DexTokenEntry, DexTokenListResponse},
        dividend::{
            DividendHolderInfo, DividendHoldersResponse, DividendRatioInfo, DividendReward,
            DividendTokenInfo, DividendTokenQuery, DividendTokensResponse,
        },
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

// Shared TokenInfo + MarketInfo column list (mirrors token::create::fetch_tokens_created).
// Requires the source token aliased `t` plus the joins in TOKEN_MARKET_JOINS.
const TOKEN_MARKET_COLUMNS: &str = r#"
    t.token_id,
    t.name as token_name,
    t.symbol as token_symbol,
    t.image_uri as token_image_uri,
    t.description as token_description,
    t.twitter as token_twitter,
    t.telegram as token_telegram,
    t.website as token_website,
    t.is_graduated,
    t.is_nsfw,
    t.is_cto,
    t.version,
    t.created_at as token_created_at,
    t.creator,
    t.token_holder_count as holder_count,
    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
    a.bio as creator_bio,
    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
    m.market_type,
    COALESCE(m.pool_id, '') as market_id,
    COALESCE(m.quote_id, '') as quote_id,
    (m.price * COALESCE(lp.price, 0)) as token_price,
    COALESCE(lp.price, 0) as native_price,
    m.price,
    (m.price * COALESCE(lp.price, 0)) as price_usd,
    t.total_supply,
    COALESCE(m.reserve_quote, 0) as reserve_quote,
    COALESCE(m.reserve_token, 0) as reserve_token,
    m.volume,
    m.ath_price,
    m.ath_price_quote,
    COALESCE(qt.name, '') as quote_name,
    COALESCE(qt.symbol, '') as quote_symbol,
    COALESCE(qt.decimals, 18) as quote_decimals,
    COALESCE(qt.image_uri, '') as quote_image_uri,
    fc.creator_fee_rate,
    fc.curve_protocol_fee_rate,
    fc.dex_protocol_fee_rate
"#;

const TOKEN_MARKET_JOINS: &str = r#"
    JOIN account a ON t.creator = a.account_id
    LEFT JOIN account_x ax ON a.account_id = ax.account_id
    JOIN market m ON t.token_id = m.token_id
    JOIN quote_token qt ON m.quote_id = qt.quote_id
    LEFT JOIN fee_config fc ON t.token_id = fc.token_id
    LEFT JOIN LATERAL (
        SELECT p.price FROM price p
        WHERE p.quote_id = m.quote_id
        ORDER BY p.block_number DESC LIMIT 1
    ) lp ON true
"#;

// Dividend-token candidate set for GET /dividend/tokens:
//   whitelist(enabled) ∪ V1(graduated) ∪ V2(all), deduped against whitelist.
// `enriched` exposes display meta + price_usd + market_cap_usd (for ordering).
// price_usd: dex_token_price (pool view) → market.price×quote→USD → price table
// (whitelist/quote tokens). Appended with a SELECT/COUNT tail before execution.
const DIVIDEND_TOKEN_CTE: &str = r#"
WITH cand AS (
    SELECT token_id, 'whitelist'::text AS token_type, sort_order
    FROM whitelist_token WHERE enabled
    UNION ALL
    SELECT token_id, 'nadfun_v1'::text AS token_type, NULL::int AS sort_order
    FROM token
    WHERE version = 'V1' AND is_graduated
      AND token_id NOT IN (SELECT token_id FROM whitelist_token WHERE enabled)
    UNION ALL
    SELECT token_id, 'nadfun_v2'::text AS token_type, NULL::int AS sort_order
    FROM token
    WHERE version = 'V2'
      AND token_id NOT IN (SELECT token_id FROM whitelist_token WHERE enabled)
),
enriched AS (
    SELECT
        c.token_id,
        c.token_type,
        c.sort_order,
        COALESCE(wl.symbol, t.symbol, dt.symbol, qt.symbol)             AS symbol,
        COALESCE(wl.name, t.name, dt.name, qt.name)                     AS name,
        COALESCE(wl.decimals, dt.decimals, qt.decimals, 18)             AS decimals,
        COALESCE(wl.image_uri, t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
        COALESCE(dtp.price_usd, m.price * lp.price, wlp.price)          AS price_usd,
        (m.price
            * (t.total_supply / POWER(10, COALESCE(dt.decimals, qt.decimals, 18))::numeric)
            * lp.price)                                                 AS market_cap_usd
    FROM cand c
    LEFT JOIN whitelist_token wl ON wl.token_id = c.token_id AND wl.enabled
    LEFT JOIN token       t  ON t.token_id  = c.token_id
    LEFT JOIN dex_token   dt ON dt.token_id = c.token_id
    LEFT JOIN quote_token qt ON qt.quote_id = c.token_id
    LEFT JOIN market      m  ON m.token_id  = c.token_id
    LEFT JOIN LATERAL (
        SELECT price FROM price WHERE quote_id = m.quote_id
        ORDER BY block_number DESC LIMIT 1
    ) lp ON true
    LEFT JOIN LATERAL (
        SELECT price FROM price WHERE quote_id = c.token_id
        ORDER BY block_number DESC LIMIT 1
    ) wlp ON true
    LEFT JOIN dex_token_price dtp ON dtp.token_id = c.token_id
)
"#;

const DIVIDEND_TOKEN_LIST_TAIL: &str = r#"
SELECT token_id, token_type, symbol, name, decimals, image_uri, price_usd
FROM enriched
WHERE ($1::varchar IS NULL OR symbol ILIKE $1 OR name ILIKE $1 OR token_id ILIKE $1)
ORDER BY
    CASE WHEN token_type = 'whitelist' THEN 0 ELSE 1 END,
    sort_order ASC NULLS LAST,
    market_cap_usd DESC NULLS LAST,
    symbol ASC NULLS LAST,
    token_id ASC
LIMIT $2 OFFSET $3
"#;

const DIVIDEND_TOKEN_COUNT_TAIL: &str = r#"
SELECT COUNT(*)::bigint AS count
FROM enriched
WHERE ($1::varchar IS NULL OR symbol ILIKE $1 OR name ILIKE $1 OR token_id ILIKE $1)
"#;

#[derive(Debug, sqlx::FromRow)]
struct TokenMarketRow {
    token_id: String,
    token_name: String,
    token_symbol: String,
    token_image_uri: String,
    token_description: Option<String>,
    token_twitter: Option<String>,
    token_telegram: Option<String>,
    token_website: Option<String>,
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    token_created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: String,
    market_id: String,
    quote_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    price_usd: BigDecimal,
    total_supply: BigDecimal,
    reserve_quote: BigDecimal,
    reserve_token: BigDecimal,
    volume: BigDecimal,
    ath_price: BigDecimal,
    ath_price_quote: BigDecimal,
    quote_name: String,
    quote_symbol: String,
    quote_decimals: i32,
    quote_image_uri: String,
    creator_fee_rate: Option<i16>,
    curve_protocol_fee_rate: Option<i16>,
    dex_protocol_fee_rate: Option<i16>,
}

fn build_token_info(row: &TokenMarketRow) -> TokenInfo {
    TokenInfo {
        token_id: row.token_id.clone(),
        name: row.token_name.clone(),
        symbol: row.token_symbol.clone(),
        image_uri: row.token_image_uri.clone(),
        description: row.token_description.clone(),
        is_graduated: row.is_graduated,
        is_nsfw: row.is_nsfw,
        twitter: row.token_twitter.clone(),
        telegram: row.token_telegram.clone(),
        website: row.token_website.clone(),
        created_at: row.token_created_at,
        creator: AccountInfo {
            account_id: row.creator.clone(),
            nickname: row.creator_nickname.clone(),
            bio: row.creator_bio.clone(),
            image_uri: row.creator_image_uri.clone(),
        },
        is_cto: row.is_cto,
        version: row.version.clone(),
    }
}

fn build_market_info(row: &TokenMarketRow) -> MarketInfo {
    let mut market_id = row.market_id.clone();
    if market_id.is_empty() {
        if row.market_type == "CURVE" {
            market_id = V1_BONDING_CURVE.clone();
        } else if row.market_type == "V2_CURVE" {
            market_id = V2_BONDING_CURVE.clone();
        }
    }

    MarketInfo {
        market_type: match row.market_type.as_str() {
            "CURVE" => MarketType::Curve,
            "DEX" => MarketType::Dex,
            "V2_CURVE" => MarketType::V2Curve,
            "V2_DEX" => MarketType::V2Dex,
            _ => MarketType::Curve,
        },
        token_id: row.token_id.clone(),
        quote_info: QuoteInfo {
            quote_id: row.quote_id.clone(),
            name: row.quote_name.clone(),
            symbol: row.quote_symbol.clone(),
            decimals: row.quote_decimals as u32,
            image_uri: row.quote_image_uri.clone(),
        },
        market_id,
        token_price: row.token_price.normalized().to_plain_string(),
        native_price: row.native_price.normalized().to_plain_string(),
        quote_price: row.native_price.normalized().to_plain_string(),
        price: row.price.normalized().to_plain_string(),
        price_usd: row.price_usd.normalized().to_plain_string(),
        price_native: row.price.normalized().to_plain_string(),
        price_quote: row.price.normalized().to_plain_string(),
        total_supply: row.total_supply.normalized().to_plain_string(),
        reserve_native: row.reserve_quote.normalized().to_plain_string(),
        reserve_quote: row.reserve_quote.normalized().to_plain_string(),
        reserve_token: row.reserve_token.normalized().to_plain_string(),
        volume: row.volume.normalized().to_plain_string(),
        ath_price: row.ath_price.normalized().to_plain_string(),
        ath_price_usd: row.ath_price.normalized().to_plain_string(),
        ath_price_native: row.ath_price_quote.normalized().to_plain_string(),
        ath_price_quote: row.ath_price_quote.normalized().to_plain_string(),
        holder_count: row.holder_count,
        fee_info: match row.market_type.as_str() {
            "V2_CURVE" | "V2_DEX" => Some(FeeInfo {
                creator_protocol_fee_rate: row.creator_fee_rate.unwrap_or(0),
                curve_protocol_fee_rate: row.curve_protocol_fee_rate.unwrap_or(0),
                dex_protocol_fee_rate: row.dex_protocol_fee_rate.unwrap_or(0),
            }),
            _ => None,
        },
    }
}

// Per dividend-token ratio (BPS) + dividend-token display metadata.
#[derive(Debug, sqlx::FromRow)]
struct DividendStatRow {
    dividend_token: String,
    ratio: i32,
    dt_name: String,
    dt_symbol: String,
    dt_decimals: i32,
    dt_image_uri: String,
}

fn dividend_quote_info(
    id: &str,
    name: &str,
    symbol: &str,
    decimals: i32,
    image_uri: &str,
) -> QuoteInfo {
    QuoteInfo {
        quote_id: id.to_string(),
        name: name.to_string(),
        symbol: symbol.to_string(),
        decimals: decimals.max(0) as u32,
        image_uri: image_uri.to_string(),
    }
}

pub struct DividendController {
    db: Arc<PostgresDatabase>,
}

impl DividendController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    // ------------------------------------------------------------------
    // Shared helpers
    // ------------------------------------------------------------------

    /// Single source-token TokenInfo + MarketInfo (used by ② and ③).
    async fn fetch_token_market(&self, token_id: &str) -> Result<TokenMarketRow> {
        let sql = format!(
            "SELECT {cols} FROM token t {joins} WHERE t.token_id = $1",
            cols = TOKEN_MARKET_COLUMNS,
            joins = TOKEN_MARKET_JOINS,
        );
        let row = measure_postgres!(
            "dividend.fetch_token_market",
            sqlx::query_as::<_, TokenMarketRow>(sql.as_str())
                .bind(token_id)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token market: {}", err))?;
        Ok(row)
    }

    /// Per dividend-token ratio (BPS) + display metadata for a source token
    /// (Trade Dividend breakdown header).
    async fn fetch_dividend_stats(&self, source_token: &str) -> Result<Vec<DividendStatRow>> {
        let rows = measure_postgres!(
            "dividend.fetch_dividend_stats",
            sqlx::query_as::<_, DividendStatRow>(
                r#"
                SELECT
                    s.dividend_token,
                    s.ratio,
                    COALESCE(qt.name, dx.name, tk.name, '') AS dt_name,
                    COALESCE(qt.symbol, dx.symbol, tk.symbol, '') AS dt_symbol,
                    COALESCE(qt.decimals, dx.decimals, 18) AS dt_decimals,
                    COALESCE(qt.image_uri, dx.image_uri, tk.image_uri, '') AS dt_image_uri
                FROM v2_dividend_setups s
                LEFT JOIN quote_token qt ON qt.quote_id = s.dividend_token
                LEFT JOIN dex_token dx ON dx.token_id = s.dividend_token
                LEFT JOIN token tk ON tk.token_id = s.dividend_token
                WHERE s.source_token = $1
                ORDER BY s.entry_index
                "#,
            )
            .bind(source_token)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend stats: {}", err))?;
        Ok(rows)
    }

    // ------------------------------------------------------------------
    // ① Profile Dividend — GET /profile/dividend/:account_id
    // ------------------------------------------------------------------

    pub async fn get_profile_dividends(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendTokensResponse> {
        let key = cache_key!(
            "dividend_profile",
            account_id,
            pagination.page,
            pagination.limit
        );
        with_cache(&GLOBAL_CACHE.cache, &key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination.clone();
            async move {
                DividendController::new(db)
                    .fetch_profile_dividends(&account_id, &pagination)
                    .await
            }
        })
        .await
    }

    async fn fetch_profile_dividends(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendTokensResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        // Query A: paged source tokens for this holder + TokenInfo/MarketInfo.
        let sql = format!(
            r#"
            WITH member_ids AS (
                SELECT source_token AS token_id FROM dividend_distribution WHERE holder = $1
                UNION
                SELECT source_token AS token_id FROM v2_dividend_claims WHERE holder = $1
            ),
            paged_tokens AS (
                SELECT t.* FROM token t
                WHERE t.token_id IN (SELECT token_id FROM member_ids)
                  AND EXISTS (SELECT 1 FROM market mk WHERE mk.token_id = t.token_id)
                ORDER BY t.created_at DESC, t.token_id ASC
                LIMIT $2 OFFSET $3
            )
            SELECT {cols} FROM paged_tokens t {joins}
            ORDER BY t.created_at DESC, t.token_id ASC
            "#,
            cols = TOKEN_MARKET_COLUMNS,
            joins = TOKEN_MARKET_JOINS,
        );
        let token_rows = measure_postgres!(
            "dividend.fetch_profile_tokens",
            sqlx::query_as::<_, TokenMarketRow>(sql.as_str())
                .bind(account_id)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch profile dividend tokens: {}", err))?;

        let source_ids: Vec<String> = token_rows.iter().map(|r| r.token_id.clone()).collect();

        // Query B: per (source, dividend) claimable leaf − claimed, with proof + meta.
        #[derive(Debug, sqlx::FromRow)]
        struct RewardRow {
            source_token: String,
            dividend_token: String,
            accrued_leaf: BigDecimal,
            proof: Vec<String>,
            claimed_amount: BigDecimal,
            claimed_usd: BigDecimal,
            last_claimed_at: Option<i64>,
            dt_name: String,
            dt_symbol: String,
            dt_decimals: i32,
            dt_image_uri: String,
        }

        let reward_rows: Vec<RewardRow> = if source_ids.is_empty() {
            Vec::new()
        } else {
            measure_postgres!(
                "dividend.fetch_profile_rewards",
                sqlx::query_as::<_, RewardRow>(
                    r#"
                    -- dividend_distribution is latest-only (PK source,holder,dividend),
                    -- so there is exactly one current-root leaf per pair.
                    WITH dist AS (
                        SELECT source_token, dividend_token, amount AS accrued_leaf, proof
                        FROM dividend_distribution
                        WHERE holder = $1 AND source_token = ANY($2::varchar[])
                    ),
                    claimed AS (
                        SELECT source_token, dividend_token,
                               SUM(amount) AS claimed_amount,
                               SUM(usd_value) AS claimed_usd,
                               MAX(created_at) AS last_claimed_at
                        FROM v2_dividend_claims
                        WHERE holder = $1 AND source_token = ANY($2::varchar[])
                        GROUP BY 1, 2
                    )
                    SELECT
                        COALESCE(d.source_token, c.source_token)     AS source_token,
                        COALESCE(d.dividend_token, c.dividend_token) AS dividend_token,
                        COALESCE(d.accrued_leaf, 0)                  AS accrued_leaf,
                        COALESCE(d.proof, ARRAY[]::text[])           AS proof,
                        COALESCE(c.claimed_amount, 0)               AS claimed_amount,
                        COALESCE(c.claimed_usd, 0)                  AS claimed_usd,
                        c.last_claimed_at,
                        COALESCE(qt.name, dx.name, tk.name, '')      AS dt_name,
                        COALESCE(qt.symbol, dx.symbol, tk.symbol, '') AS dt_symbol,
                        COALESCE(qt.decimals, dx.decimals, 18)       AS dt_decimals,
                        COALESCE(qt.image_uri, dx.image_uri, tk.image_uri, '') AS dt_image_uri
                    FROM dist d
                    FULL OUTER JOIN claimed c USING (source_token, dividend_token)
                    LEFT JOIN quote_token qt ON qt.quote_id = COALESCE(d.dividend_token, c.dividend_token)
                    LEFT JOIN dex_token dx ON dx.token_id = COALESCE(d.dividend_token, c.dividend_token)
                    LEFT JOIN token tk ON tk.token_id = COALESCE(d.dividend_token, c.dividend_token)
                    "#,
                )
                .bind(account_id)
                .bind(&source_ids)
                .fetch_all(self.db.get_read_pool())
            )
            .map_err(|err| anyhow!("Failed to fetch profile dividend rewards: {}", err))?
        };

        // Group rewards by source token.
        let mut rewards_by_source: HashMap<String, Vec<DividendReward>> = HashMap::new();
        let mut last_claimed_by_source: HashMap<String, i64> = HashMap::new();
        let mut claimed_usd_by_source: HashMap<String, BigDecimal> = HashMap::new();
        for r in reward_rows {
            if let Some(ts) = r.last_claimed_at {
                let slot = last_claimed_by_source.entry(r.source_token.clone()).or_insert(ts);
                if ts > *slot {
                    *slot = ts;
                }
            }
            // Accumulate the token-row claimed-USD total (Σ over dividend tokens).
            let usd_slot = claimed_usd_by_source
                .entry(r.source_token.clone())
                .or_insert_with(|| BigDecimal::from(0));
            *usd_slot = usd_slot.clone() + r.claimed_usd.clone();
            let claimable = r.accrued_leaf > r.claimed_amount;
            let reward = DividendReward {
                dividend_token_info: dividend_quote_info(
                    &r.dividend_token,
                    &r.dt_name,
                    &r.dt_symbol,
                    r.dt_decimals,
                    &r.dt_image_uri,
                ),
                reward_info: RewardInfo {
                    amount: r.accrued_leaf.normalized().to_plain_string(),
                    claimed_amount: r.claimed_amount.normalized().to_plain_string(),
                    proof: r.proof,
                    claimable,
                },
                claimed_usd: r.claimed_usd.normalized().to_plain_string(),
            };
            rewards_by_source
                .entry(r.source_token)
                .or_default()
                .push(reward);
        }

        let tokens: Vec<DividendTokenInfo> = token_rows
            .into_iter()
            .map(|row| {
                let rewards = rewards_by_source.remove(&row.token_id).unwrap_or_default();
                let last_claimed_at = last_claimed_by_source.get(&row.token_id).copied();
                let claimed_usd = claimed_usd_by_source
                    .get(&row.token_id)
                    .map(|v| v.normalized().to_plain_string())
                    .unwrap_or_else(|| "0".to_string());
                DividendTokenInfo {
                    token_info: build_token_info(&row),
                    market_info: build_market_info(&row),
                    rewards,
                    last_claimed_at,
                    claimed_usd,
                }
            })
            .collect();

        let total_count = measure_postgres!(
            "dividend.fetch_profile_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(*)::bigint AS count FROM (
                    SELECT source_token FROM dividend_distribution WHERE holder = $1
                    UNION
                    SELECT source_token FROM v2_dividend_claims WHERE holder = $1
                ) s
                WHERE EXISTS (SELECT 1 FROM token t WHERE t.token_id = s.source_token)
                  AND EXISTS (SELECT 1 FROM market mk WHERE mk.token_id = s.source_token)
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch profile dividend count: {}", err))?;

        Ok(DividendTokensResponse {
            tokens,
            total_count: total_count.count,
        })
    }

    // ------------------------------------------------------------------
    // ② Trade Dividend — GET /dividend/holders/:token_id
    // ------------------------------------------------------------------

    pub async fn get_dividend_holders(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendHoldersResponse> {
        let key = cache_key!(
            "dividend_holders",
            token_id,
            pagination.page,
            pagination.limit
        );
        with_cache(&GLOBAL_CACHE.cache, &key, || {
            let db = self.db.clone();
            let token_id = token_id.to_string();
            let pagination = pagination.clone();
            async move {
                DividendController::new(db)
                    .fetch_dividend_holders(&token_id, &pagination)
                    .await
            }
        })
        .await
    }

    async fn fetch_dividend_holders(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendHoldersResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let token_info = build_token_info(&self.fetch_token_market(token_id).await?);

        let dividend_tokens: Vec<DividendRatioInfo> = self
            .fetch_dividend_stats(token_id)
            .await?
            .into_iter()
            .map(|s| DividendRatioInfo {
                dividend_token_info: dividend_quote_info(
                    &s.dividend_token,
                    &s.dt_name,
                    &s.dt_symbol,
                    s.dt_decimals,
                    &s.dt_image_uri,
                ),
                ratio_bps: s.ratio,
            })
            .collect();

        #[derive(Debug, sqlx::FromRow)]
        struct HolderRankRow {
            holder: String,
            total_value_usd: BigDecimal,
            last_received_at: i64,
            nickname: Option<String>,
            bio: Option<String>,
            image_uri: Option<String>,
        }

        let holder_rows = measure_postgres!(
            "dividend.fetch_dividend_holders",
            sqlx::query_as::<_, HolderRankRow>(
                r#"
                WITH acc AS (
                    SELECT a.holder, a.dividend_token, a.accrued, a.updated_at
                    FROM dividend_accrual a
                    WHERE a.source_token = $1 AND a.accrued > 0
                ),
                valued AS (
                    SELECT h.holder,
                           MAX(h.updated_at) AS last_received_at,
                           COALESCE(SUM(
                               h.accrued
                               / POWER(10, COALESCE(qt.decimals, dx.decimals, 18))::numeric
                               * COALESCE(
                                   (SELECT pr.price FROM price pr
                                      WHERE pr.quote_id = h.dividend_token
                                      ORDER BY pr.block_number DESC LIMIT 1),
                                   (SELECT dtp.price_usd FROM dex_token_price dtp
                                      WHERE dtp.token_id = h.dividend_token),
                                   0)
                           ), 0) AS total_value_usd
                    FROM acc h
                    LEFT JOIN quote_token qt ON qt.quote_id = h.dividend_token
                    LEFT JOIN dex_token dx ON dx.token_id = h.dividend_token
                    GROUP BY h.holder
                )
                SELECT v.holder, v.total_value_usd, v.last_received_at,
                       acct.nickname, acct.bio, acct.image_uri
                FROM valued v
                LEFT JOIN account acct ON acct.account_id = v.holder
                ORDER BY v.total_value_usd DESC, v.holder ASC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(token_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend holders: {}", err))?;

        let holders: Vec<DividendHolderInfo> = holder_rows
            .into_iter()
            .map(|r| DividendHolderInfo {
                holder: AccountInfo {
                    account_id: r.holder.clone(),
                    nickname: r.nickname.unwrap_or_else(|| r.holder.clone()),
                    bio: r.bio.unwrap_or_default(),
                    image_uri: r.image_uri.unwrap_or_default(),
                },
                total_value_usd: r.total_value_usd.normalized().to_plain_string(),
                last_received_at: r.last_received_at,
            })
            .collect();

        let total_count = measure_postgres!(
            "dividend.fetch_dividend_holders_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT holder)::bigint AS count
                FROM dividend_accrual
                WHERE source_token = $1 AND accrued > 0
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend holders count: {}", err))?;

        Ok(DividendHoldersResponse {
            token_info,
            dividend_tokens,
            holders,
            total_count: total_count.count,
        })
    }

    // ------------------------------------------------------------------
    // Dividend token search — GET /dividend/tokens
    //   candidates: whitelist(enabled) ∪ V1(graduated) ∪ V2(all)
    // ------------------------------------------------------------------

    pub async fn get_dividend_tokens(
        &self,
        query: &DividendTokenQuery,
    ) -> Result<DexTokenListResponse> {
        let key = cache_key!(
            "dividend_tokens",
            query.q.clone().unwrap_or_default(),
            query.page,
            query.limit
        );
        with_cache(&GLOBAL_CACHE.cache, &key, || {
            let db = self.db.clone();
            let query = query.clone();
            async move {
                DividendController::new(db)
                    .fetch_dividend_tokens(&query)
                    .await
            }
        })
        .await
    }

    async fn fetch_dividend_tokens(
        &self,
        query: &DividendTokenQuery,
    ) -> Result<DexTokenListResponse> {
        let offset = (query.page - 1) * query.limit;
        // None → full list (WHERE short-circuits on $1 IS NULL); else substring ILIKE.
        let pattern: Option<String> = query
            .q
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{}%", s));

        #[derive(Debug, sqlx::FromRow)]
        struct Row {
            token_id: String,
            token_type: String,
            symbol: Option<String>,
            name: Option<String>,
            decimals: Option<i32>,
            image_uri: Option<String>,
            price_usd: Option<BigDecimal>,
        }

        let list_sql = format!("{}{}", DIVIDEND_TOKEN_CTE, DIVIDEND_TOKEN_LIST_TAIL);
        let rows = measure_postgres!(
            "dividend.fetch_dividend_tokens",
            sqlx::query_as::<_, Row>(&list_sql)
                .bind(&pattern)
                .bind(query.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend tokens: {}", err))?;

        let count_sql = format!("{}{}", DIVIDEND_TOKEN_CTE, DIVIDEND_TOKEN_COUNT_TAIL);
        let total = measure_postgres!(
            "dividend.fetch_dividend_tokens_count",
            sqlx::query_as::<_, CountRow>(&count_sql)
                .bind(&pattern)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to count dividend tokens: {}", err))?;

        let tokens = rows
            .into_iter()
            .map(|r| DexTokenEntry {
                token_id: r.token_id,
                symbol: r.symbol.unwrap_or_default(),
                name: r.name.unwrap_or_default(),
                decimals: r.decimals.unwrap_or(18),
                image_uri: r.image_uri.unwrap_or_default(),
                token_type: r.token_type,
                balance: None,
                balance_usd: None,
                price_usd: r.price_usd.map(|p| p.normalized().to_plain_string()),
            })
            .collect();

        Ok(DexTokenListResponse {
            tokens,
            total_count: total.count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::pagination::PaginationParams;
    use sqlx::PgPool;

    const CREATOR: &str = "0x000000000000000000000000000000000000Aa09";
    const HOLDER: &str = "0x000000000000000000000000000000000000Aa01";
    const HOLDER2: &str = "0x000000000000000000000000000000000000Aa02";
    const TOKEN: &str = "0x000000000000000000000000000000000000Bb01";
    const QUOTE: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A"; // MON (seeded)

    fn ctrl(pool: PgPool) -> DividendController {
        DividendController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    fn page() -> PaginationParams {
        PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        }
    }

    /// account(creator) + source token + V2_DEX market(MON) + MON price.
    async fn seed_base(pool: &PgPool) {
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'creator','','') ON CONFLICT DO NOTHING")
            .bind(CREATOR).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'Beak','BEAK','',$2,NULL,false,true,false,100,'0xh',1000000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(CREATOR).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('V2_DEX',$1,NULL,0,0,1,$2,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#)
            .bind(TOKEN).bind(QUOTE).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO price (quote_id,block_number,price) VALUES ($1,1,2) ON CONFLICT DO NOTHING")
            .bind(QUOTE).execute(pool).await.unwrap();
    }

    // ① Profile Dividend: claimable = leaf − claimed, with proof + last_claimed_at.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn profile_dividend_claimable(pool: PgPool) {
        seed_base(&pool).await;
        sqlx::query("INSERT INTO dividend_distribution (merkle_root,source_token,holder,dividend_token,amount,proof,status,created_at) VALUES ('0xroot',$1,$2,$3,100,ARRAY['0xa','0xb'],'AWAITING',10)")
            .bind(TOKEN).bind(HOLDER).bind(QUOTE).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO v2_dividend_claims (holder,source_token,dividend_token,amount,merkle_root,entry_index,transaction_hash,block_number,created_at,log_index,tx_index,usd_value) VALUES ($1,$2,$3,30,'0xroot',0,'0xc1',1,20,0,0,25)"#)
            .bind(HOLDER).bind(TOKEN).bind(QUOTE).execute(&pool).await.unwrap();

        let resp = ctrl(pool).fetch_profile_dividends(HOLDER, &page()).await.unwrap();

        assert_eq!(resp.total_count, 1);
        assert_eq!(resp.tokens.len(), 1);
        let t = &resp.tokens[0];
        assert_eq!(t.token_info.token_id, TOKEN);
        assert_eq!(t.last_claimed_at, Some(20));
        assert_eq!(t.claimed_usd, "25"); // token-row total = Σ rewards[].claimed_usd
        assert_eq!(t.rewards.len(), 1);
        let r = &t.rewards[0];
        assert_eq!(r.dividend_token_info.symbol, "MON");
        assert_eq!(r.reward_info.amount, "100");
        assert_eq!(r.reward_info.claimed_amount, "30");
        assert_eq!(r.claimed_usd, "25"); // per dividend token claimed USD
        assert!(r.reward_info.claimable); // 100 > 30
        assert_eq!(r.reward_info.proof, vec!["0xa".to_string(), "0xb".to_string()]);
    }

    // ② Trade Dividend: holders ranked by cumulative accrued USD.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn trade_dividend_holder_ranking(pool: PgPool) {
        seed_base(&pool).await;
        for (a, n) in [(HOLDER, "alice"), (HOLDER2, "bob")] {
            sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,$2,'','') ON CONFLICT DO NOTHING")
                .bind(a).bind(n).execute(&pool).await.unwrap();
        }
        sqlx::query(r#"INSERT INTO v2_dividend_setups (source_token,dividend_token,ratio,min_balance,entry_index,transaction_hash,block_number,created_at,log_index,tx_index) VALUES ($1,$2,10000,0,0,'0xs1',1,1,0,0)"#)
            .bind(TOKEN).bind(QUOTE).execute(&pool).await.unwrap();
        // accrued in raw 1e18 units; /1e18 * price(2) => USD.
        sqlx::query("INSERT INTO dividend_accrual (source_token,holder,dividend_token,accrued,updated_at) VALUES ($1,$2,$3,10000000000000000000,100)")
            .bind(TOKEN).bind(HOLDER).bind(QUOTE).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO dividend_accrual (source_token,holder,dividend_token,accrued,updated_at) VALUES ($1,$2,$3,5000000000000000000,90)")
            .bind(TOKEN).bind(HOLDER2).bind(QUOTE).execute(&pool).await.unwrap();

        let resp = ctrl(pool).fetch_dividend_holders(TOKEN, &page()).await.unwrap();

        assert_eq!(resp.token_info.token_id, TOKEN);
        assert_eq!(resp.dividend_tokens.len(), 1);
        assert_eq!(resp.dividend_tokens[0].ratio_bps, 10000);
        assert_eq!(resp.total_count, 2);
        assert_eq!(resp.holders.len(), 2);
        // alice (10 * 2 = 20) ranks above bob (5 * 2 = 10).
        assert_eq!(resp.holders[0].holder.account_id, HOLDER);
        assert_eq!(resp.holders[0].holder.nickname, "alice");
        assert_eq!(resp.holders[0].total_value_usd, "20");
        assert_eq!(resp.holders[0].last_received_at, 100);
        assert_eq!(resp.holders[1].holder.account_id, HOLDER2);
        assert_eq!(resp.holders[1].total_value_usd, "10");
    }

    const WLTOKEN: &str = "0x000000000000000000000000000000000000Dd01";
    const V1TOKEN: &str = "0x000000000000000000000000000000000000Dd02";
    const V1NONGRAD: &str = "0x000000000000000000000000000000000000Dd03";

    // Dividend token search: candidate set = whitelist ∪ V1-graduated ∪ V2;
    // non-graduated V1 must be excluded.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn dividend_token_search(pool: PgPool) {
        seed_base(&pool).await; // TOKEN = V2 (graduated)
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'OldCoin','OLD','',$2,NULL,false,true,false,50,'0xh',1000000,'V1') ON CONFLICT DO NOTHING"#)
            .bind(V1TOKEN).bind(CREATOR).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'CurveCoin','CRV','',$2,NULL,false,false,false,40,'0xh',1000000,'V1') ON CONFLICT DO NOTHING"#)
            .bind(V1NONGRAD).bind(CREATOR).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO whitelist_token (token_id,sort_order,enabled,name,symbol) VALUES ($1,99,true,'USD Coin','USDC') ON CONFLICT DO NOTHING")
            .bind(WLTOKEN).execute(&pool).await.unwrap();

        let c = ctrl(pool);

        // full candidate list
        let all = c
            .fetch_dividend_tokens(&DividendTokenQuery { q: None, page: 1, limit: 50 })
            .await
            .unwrap();
        let ids: Vec<&str> = all.tokens.iter().map(|t| t.token_id.as_str()).collect();
        assert!(ids.contains(&WLTOKEN), "whitelist token present");
        assert!(ids.contains(&TOKEN), "V2 token present");
        assert!(ids.contains(&V1TOKEN), "V1 graduated token present");
        assert!(!ids.contains(&V1NONGRAD), "V1 non-graduated excluded");
        assert_eq!(
            all.tokens.iter().find(|t| t.token_id == WLTOKEN).unwrap().token_type,
            "whitelist"
        );
        assert_eq!(
            all.tokens.iter().find(|t| t.token_id == V1TOKEN).unwrap().token_type,
            "nadfun_v1"
        );
        assert_eq!(
            all.tokens.iter().find(|t| t.token_id == TOKEN).unwrap().token_type,
            "nadfun_v2"
        );

        // search by symbol
        let found = c
            .fetch_dividend_tokens(&DividendTokenQuery {
                q: Some("USDC".into()),
                page: 1,
                limit: 50,
            })
            .await
            .unwrap();
        assert_eq!(found.total_count, 1);
        assert_eq!(found.tokens.len(), 1);
        assert_eq!(found.tokens[0].token_id, WLTOKEN);
    }
}
