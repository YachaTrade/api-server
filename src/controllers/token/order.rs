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
                AccountInfo, FeeInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo, TokenVersion,
            },
            pagination::PaginationParams,
        },
        token::order::{OrderToken, OrderTokenResponse, TokenOrderType},
    },
    utils::{
        calculate_price_change_percent, current_unix_timestamp,
        single_flight::{GLOBAL_CACHE, with_cache},
    },
};

#[derive(Debug, sqlx::FromRow)]
struct OrderTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    created_at: i64,
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
    price_24h_ago: BigDecimal,
    creator_fee_rate: Option<i16>,
    curve_protocol_fee_rate: Option<i16>,
    dex_protocol_fee_rate: Option<i16>,
}

/// latest_trade 더스트 필터 임계값: swap.quote_amount(네이티브/MON 측)이
/// 이 값(0.1 MON = 10^17 wei) 이상인 거래만 "최근 거래"로 인정한다.
/// 미만만 있는(또는 거래가 없는) 토큰은 latest_trade 목록에서 제외된다.
const LATEST_TRADE_MIN_QUOTE_AMOUNT: i64 = 100_000_000_000_000_000;

pub struct OrderController {
    db: Arc<PostgresDatabase>,
}

impl OrderController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        OrderController { db }
    }

    pub async fn get_order_tokens(
        &self,
        order_by: TokenOrderType,
        pagination: &PaginationParams,
        is_nsfw: bool,
    ) -> Result<Vec<OrderToken>> {
        let cache_key = cache_key!(
            "order_tokens",
            order_by.as_str(),
            pagination.direction,
            pagination.page,
            pagination.limit,
            is_nsfw
        );

        let order_by_clone = order_by;
        let tokens = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async move {
            self.fetch_order_tokens(order_by_clone, pagination, is_nsfw)
                .await
        })
        .await?;

        Ok(tokens)
    }

    async fn fetch_order_tokens(
        &self,
        order_by: TokenOrderType,
        pagination: &PaginationParams,
        is_nsfw: bool,
    ) -> Result<Vec<OrderToken>> {
        let offset = (pagination.page - 1) * pagination.limit;
        let order_direction = &pagination.direction;

        let rows = match order_by {
            TokenOrderType::CreationTime => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
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
                        fc.dex_protocol_fee_rate,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON t.token_id = m.token_id
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
                    LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    WHERE {}
                    ORDER BY t.created_at {}
                    LIMIT $1 OFFSET $2
                    "#,
                    nsfw_filter, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_creation_time",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by creation time: {}", err))?
            }
            TokenOrderType::LatestTrade => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    WITH ranked AS (
                        -- 정렬키(last_trade_at)만 토큰별로 계산해 먼저 페이지(≤LIMIT)를 확정.
                        -- 무거운 account/quote/fee/price_history join은 확정된 행에만 적용해
                        -- 전 토큰(~수만)이 아니라 페이지 크기만큼만 조회한다(join 지연 최적화).
                        SELECT t.token_id, lt.last_trade_at
                        FROM market m
                        JOIN token t ON t.token_id = m.token_id
                        JOIN LATERAL (
                            -- 최근 거래 = swap 기준. quote_amount(네이티브/MON 측)이
                            -- 0.1 MON(10^17 wei) 이상인 swap만 "거래"로 인정한다.
                            -- 자격 swap이 없는 토큰은 INNER JOIN이라 목록에서 제외된다.
                            SELECT s.created_at AS last_trade_at
                            FROM swap s
                            WHERE s.token_id = t.token_id
                              AND s.quote_amount >= {}
                            ORDER BY s.created_at DESC
                            LIMIT 1
                        ) lt ON true
                        WHERE {}
                        ORDER BY lt.last_trade_at {}
                        LIMIT $1 OFFSET $2
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
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
                        fc.dex_protocol_fee_rate,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    -- ranked에서 확정된 페이지에만 메타 join 적용. account/quote/fee는
                    -- FK 보장 INNER라 행을 줄이지 않으므로 지연해도 결과 동일.
                    FROM ranked r
                    JOIN token t        ON t.token_id = r.token_id
                    JOIN market m       ON m.token_id = r.token_id
                    JOIN account a      ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
                    LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    ORDER BY r.last_trade_at {}
                    "#,
                    LATEST_TRADE_MIN_QUOTE_AMOUNT, nsfw_filter, order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_latest_trade",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by latest trade: {}", err))?
            }
            TokenOrderType::MarketCap => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
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
                        fc.dex_protocol_fee_rate,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    FROM (
                        SELECT m.token_id, m.price, m.market_type, m.pool_id, m.quote_id, m.reserve_quote, m.reserve_token, m.volume, m.ath_price, m.ath_price_quote
                        FROM market m
                        JOIN token t ON m.token_id = t.token_id
                        WHERE {}
                        ORDER BY m.price {}
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
                    LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    ORDER BY m.price {}
                    "#,
                    nsfw_filter, order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_market_cap",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by market cap: {}", err))?
            }
        };

        Ok(rows.into_iter().map(OrderToken::from).collect())
    }

    pub async fn get_total_count_by_type(
        &self,
        order_type: &TokenOrderType,
        is_nsfw: bool,
    ) -> Result<i64> {
        // NOTE: latest_trade는 listing에서 swap 기준(quote_amount >= 0.1 MON)으로 토큰을
        // 추가 필터하지만, total_count는 비싼 COUNT(DISTINCT swap)(수백 ms) 대신 denormalized
        // token_count(트리거 유지, O(1))를 그대로 쓴다. 자격 미달 토큰(~2.6%)만큼 약간
        // 과대계상되나 페이지네이션 용도로 충분하다.
        match order_type {
            _ => {
                // is_nsfw = true: return all tokens (total_count)
                // is_nsfw = false: return only SFW tokens (sfw_count)
                let column = if is_nsfw { "total_count" } else { "sfw_count" };
                let query = format!("SELECT {} as count FROM token_count", column);

                let row = measure_postgres!(
                    "token_order.get_total_count_by_type",
                    sqlx::query_as::<_, CountRow>(&query).fetch_one(self.db.get_read_pool())
                )
                .map_err(|e| anyhow!("Failed to get total_count: {}", e))?;

                Ok(row.count)
            }
        }
    }

    pub fn build_order_response(tokens: Vec<OrderToken>, total_count: i64) -> OrderTokenResponse {
        OrderTokenResponse {
            tokens,
            total_count,
        }
    }
}

impl From<OrderTokenRow> for OrderToken {
    fn from(row: OrderTokenRow) -> Self {
        let mut market_id = row.market_id.clone();
        if market_id.is_empty() {
            if row.market_type == "CURVE" {
                market_id = V1_BONDING_CURVE.clone();
            } else if row.market_type == "V2_CURVE" {
                market_id = V2_BONDING_CURVE.clone();
            }
        }

        let percent = calculate_price_change_percent(
            &row.price_24h_ago.normalized().to_plain_string(),
            &row.price.normalized().to_plain_string(),
        )
        .unwrap_or(0.0);

        OrderToken {
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_graduated: row.is_graduated,
                is_nsfw: row.is_nsfw,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
                created_at: row.created_at,
                creator: AccountInfo {
                    account_id: row.creator,
                    nickname: row.creator_nickname,
                    bio: row.creator_bio,
                    image_uri: row.creator_image_uri,
                },
                is_cto: row.is_cto,
                version: row.version.clone(),
                x_verification: None,
            },
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    "V2_CURVE" => MarketType::V2Curve,
                    "V2_DEX" => MarketType::V2Dex,
                    _ => MarketType::Curve,
                },
                token_id: row.token_id,
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
            },
            percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// 40-hex-padded 주소 (DB는 VARCHAR(42)라 체크섬 무관).
    fn addr(suffix: &str) -> String {
        format!("0x{:0>40}", suffix)
    }

    fn make_controller(pool: PgPool) -> OrderController {
        OrderController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_account(pool: &PgPool, account_id: &str) {
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri)
             VALUES ($1, 'n', '', '') ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_quote(pool: &PgPool, quote_id: &str) {
        sqlx::query(
            "INSERT INTO quote_token (quote_id, name, symbol, decimals, pyth_feed_id, image_uri)
             VALUES ($1, 'Monad', 'MON', 18, 'feed', '')
             ON CONFLICT (quote_id) DO NOTHING",
        )
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_token_market(pool: &PgPool, token_id: &str, creator: &str, quote_id: &str) {
        sqlx::query(
            "INSERT INTO token
                (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply, version)
             VALUES ($1, 'n', 's', '', $2, 0, $3, 0, 'V2')
             ON CONFLICT (token_id) DO NOTHING",
        )
        .bind(token_id)
        .bind(creator)
        .bind(format!("txtok_{token_id}"))
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO market (market_type, token_id, price, quote_id, latest_trade_at, created_at)
             VALUES ('V2_DEX', $1, 1, $2, 0, 0)
             ON CONFLICT (token_id) DO NOTHING",
        )
        .bind(token_id)
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_swap(
        pool: &PgPool,
        account_id: &str,
        token_id: &str,
        quote_amount: &str,
        created_at: i64,
        tx: &str,
    ) {
        sqlx::query(
            "INSERT INTO swap
                (account_id, token_id, market_type, is_buy, quote_amount, token_amount,
                 created_at, transaction_hash, tx_index, log_index)
             VALUES ($1, $2, 'V2_DEX', true, $3::NUMERIC, 0, $4, $5, 0, 0)",
        )
        .bind(account_id)
        .bind(token_id)
        .bind(quote_amount)
        .bind(created_at)
        .bind(tx)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_price_history(pool: &PgPool, token_id: &str, block_number: i64) {
        sqlx::query(
            "INSERT INTO price_history
                (token_id, price, volume, created_at, block_number, transaction_hash, tx_index, log_index)
             VALUES ($1, 1, 0, 0, $2, $3, 0, 0)",
        )
        .bind(token_id)
        .bind(block_number)
        .bind(format!("txph_{token_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    /// latest_trade는 swap 기준(quote_amount >= 0.1 MON = 10^17 wei):
    /// - 자격 swap이 있는 토큰만 노출(없으면 제외)
    /// - 더스트(임계 미만) swap은 "최근 거래"로 치지 않음 → 정렬에 영향 없음
    /// - total_count도 동일 기준으로 재계산
    #[sqlx::test(migrations = "./migrations-test")]
    async fn latest_trade_filters_by_quote_amount_and_orders_by_qualifying_swap(pool: PgPool) {
        let acc = addr("acc1");
        let quote = addr("9011");
        let t_big = addr("b161"); // 자격 거래 created_at=1000, 이후 더스트 created_at=5000
        let t_big2 = addr("b162"); // 자격 거래 created_at=2000 (더 최신)
        let t_dust = addr("d057"); // 더스트만 → 제외
        let t_none = addr("0e0e"); // 거래 없음 → 제외

        seed_account(&pool, &acc).await;
        seed_quote(&pool, &quote).await;
        for t in [&t_big, &t_big2, &t_dust, &t_none] {
            seed_token_market(&pool, t, &acc, &quote).await;
        }
        // 반환되는 토큰만 price_history 필요 (price_24h_ago는 non-Option 디코드).
        seed_price_history(&pool, &t_big, 1).await;
        seed_price_history(&pool, &t_big2, 2).await;

        seed_swap(&pool, &acc, &t_big, "100000000000000000", 1000, "big_q").await; // == 임계 → 자격
        seed_swap(&pool, &acc, &t_big, "50000000000000000", 5000, "big_d").await; // 더스트(최신이나 무시)
        seed_swap(&pool, &acc, &t_big2, "200000000000000000", 2000, "big2_q").await; // 자격
        seed_swap(&pool, &acc, &t_dust, "90000000000000000", 9000, "dust").await; // 더스트만

        let controller = make_controller(pool);
        let pagination = PaginationParams {
            page: 1,
            limit: 50,
            direction: "DESC".to_string(),
        };

        let rows = controller
            // fetch_order_tokens 직접 호출 — 프로세스 전역 GLOBAL_CACHE 오염 회피.
            .fetch_order_tokens(TokenOrderType::LatestTrade, &pagination, false)
            .await
            .unwrap();

        let ids: Vec<&str> = rows
            .iter()
            .map(|r| r.token_info.token_id.as_str())
            .collect();

        // 자격 거래 시각 DESC: t_big2(2000) → t_big(1000). 더스트 created_at=5000은 무시.
        assert_eq!(
            ids,
            vec![t_big2.as_str(), t_big.as_str()],
            "자격 거래(>=0.1 MON) 있는 토큰만, 그 거래 시각 DESC로 정렬"
        );
        assert!(!ids.contains(&t_dust.as_str()), "더스트만 있는 토큰은 제외");
        assert!(!ids.contains(&t_none.as_str()), "거래 없는 토큰은 제외");

        // total_count는 denormalized token_count(sfw_count) — listing의 swap 필터와
        // 무관하게 전체 SFW 토큰 수(여기선 4). 약간의 과대계상은 의도된 동작(O(1) 우선).
        let count = controller
            .get_total_count_by_type(&TokenOrderType::LatestTrade, false)
            .await
            .unwrap();
        assert_eq!(
            count, 4,
            "total_count = token_count.sfw_count(전체 SFW 4), swap 필터 무관"
        );
    }
}
