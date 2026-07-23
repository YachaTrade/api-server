use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{AccountInfo, MarketInfo, MarketType, QuoteInfo, TokenInfo},
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
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: MarketType,
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
}

/// latest_trade에서 최근 거래로 인정할 최소 swap.quote_amount.
const LATEST_TRADE_MIN_QUOTE_AMOUNT: i64 = 10;

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
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        a.nickname as creator_nickname,
                        a.bio as creator_bio,
                        a.image_uri as creator_image_uri,
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
                    JOIN market m ON t.token_id = m.token_id
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
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
                            -- quote_amount가 10 wei 이상인 가장 최근 swap을
                            -- "최근 거래"로 인정한다. 자격 swap이 없는 토큰은 제외된다.
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
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        a.nickname as creator_nickname,
                        a.bio as creator_bio,
                        a.image_uri as creator_image_uri,
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
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
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
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        a.nickname as creator_nickname,
                        a.bio as creator_bio,
                        a.image_uri as creator_image_uri,
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
                    JOIN quote_token qt ON m.quote_id = qt.quote_id
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
        // NOTE: latest_trade는 quote_amount >= 10 wei인 swap이 없는 토큰을 제외하지만,
        // total_count는 비싼 COUNT(DISTINCT swap) 대신 denormalized token_count
        // (트리거 유지, O(1))를 그대로 쓴다. 자격 거래가 없는 토큰만큼
        // 과대계상될 수 있다.
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
        if market_id.is_empty() && row.market_type == MarketType::Curve {
            market_id = BONDING_CURVE.clone();
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
            },
            market_info: MarketInfo {
                market_type: row.market_type,
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
             VALUES ($1, 'Wrapped Ether', 'WETH', 18, 'feed', '')
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
                (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply)
             VALUES ($1, 'n', 's', '', $2, 0, $3, 0)
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
             VALUES ('DEX', $1, 1, $2, 0, 0)
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
             VALUES ($1, $2, 'DEX', true, $3::NUMERIC, 0, $4, $5, 0, 0)",
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

    /// latest_trade는 swap.quote_amount >= 10 wei 기준으로:
    /// - 자격 swap이 있는 토큰만 노출(없으면 제외)
    /// - 가장 최근 자격 swap 시각으로 정렬
    /// - total_count는 swap 필터와 무관한 token_count 값을 사용
    #[sqlx::test(migrations = "./migrations")]
    async fn latest_trade_filters_by_minimum_amount_and_orders_by_qualifying_swap(pool: PgPool) {
        let acc = addr("acc1");
        let quote = addr("9011");
        let t_big = addr("b161"); // 자격 거래 created_at=1000, 이후 9 wei 거래
        let t_big2 = addr("b162"); // 자격 거래 created_at=2000
        let t_small = addr("d057"); // 9 wei 거래만 있어 제외
        let t_none = addr("0e0e"); // 거래 없음 → 제외

        seed_account(&pool, &acc).await;
        seed_quote(&pool, &quote).await;
        for t in [&t_big, &t_big2, &t_small, &t_none] {
            seed_token_market(&pool, t, &acc, &quote).await;
        }
        // 반환되는 토큰만 price_history 필요 (price_24h_ago는 non-Option 디코드).
        seed_price_history(&pool, &t_big, 1).await;
        seed_price_history(&pool, &t_big2, 2).await;

        seed_swap(&pool, &acc, &t_big, "10", 1000, "big_q").await;
        seed_swap(&pool, &acc, &t_big, "9", 5000, "big_small").await;
        seed_swap(&pool, &acc, &t_big2, "2000", 2000, "big2_q").await;
        seed_swap(&pool, &acc, &t_small, "9", 9000, "small").await;

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

        // 최근 자격 거래 시각 DESC: t_big2(2000) → t_big(1000).
        // 9 wei 거래는 더 최신이어도 무시한다.
        assert_eq!(
            ids,
            vec![t_big2.as_str(), t_big.as_str()],
            "10 wei 이상 swap이 있는 토큰을 최근 자격 거래 시각 DESC로 정렬"
        );
        assert!(
            !ids.contains(&t_small.as_str()),
            "10 wei 미만 거래만 있으면 제외"
        );
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
