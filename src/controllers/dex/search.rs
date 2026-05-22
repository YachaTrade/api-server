use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::{
        search::DexSearchQuery,
        tokens::{DexTokenEntry, DexTokenListResponse},
    },
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, sqlx::FromRow)]
struct SearchRow {
    token_id: String,
    symbol: Option<String>,
    name: Option<String>,
    decimals: Option<i32>,
    image_uri: Option<String>,
    balance: Option<BigDecimal>,
    market_cap_usd: Option<BigDecimal>,
}

pub struct SearchController {
    db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Session-authenticated token search. `account_id` is the session.address —
    /// always present (the route requires `authenticate_user` middleware).
    pub async fn search_tokens(
        &self,
        query: &DexSearchQuery,
        account_id: &str,
    ) -> Result<DexTokenListResponse> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = query.offset.unwrap_or(0).max(0);
        let fetch_n = limit + 1;

        let rows = measure_postgres!(
            "dex.search_tokens",
            sqlx::query_as::<_, SearchRow>(
                r#"
                WITH tokens_in_pools AS (
                    SELECT token0 AS token_id FROM pool
                    UNION
                    SELECT token1 AS token_id FROM pool
                )
                SELECT
                    tp.token_id,
                    COALESCE(t.symbol,    dt.symbol,    qt.symbol)    AS symbol,
                    COALESCE(t.name,      dt.name,      qt.name)      AS name,
                    COALESCE(dt.decimals, qt.decimals)                AS decimals,
                    COALESCE(t.image_uri, dt.image_uri, qt.image_uri) AS image_uri,
                    b.balance                                         AS balance,
                    (m.price * t.total_supply * lp.price)             AS market_cap_usd
                FROM tokens_in_pools tp
                LEFT JOIN token       t   ON t.token_id  = tp.token_id
                LEFT JOIN dex_token   dt  ON dt.token_id = tp.token_id
                LEFT JOIN quote_token qt  ON qt.quote_id = tp.token_id
                LEFT JOIN balance     b   ON b.token_id  = tp.token_id
                                         AND b.account_id = $1
                LEFT JOIN market      m   ON m.token_id  = tp.token_id
                LEFT JOIN LATERAL (
                    SELECT price FROM price
                     WHERE quote_id = m.quote_id
                     ORDER BY block_number DESC
                     LIMIT 1
                ) lp ON true
                WHERE
                    COALESCE(t.symbol,    dt.symbol,    qt.symbol) ILIKE '%' || $2 || '%'
                 OR COALESCE(t.name,      dt.name,      qt.name)   ILIKE '%' || $2 || '%'
                 OR tp.token_id                                    ILIKE '%' || $2 || '%'
                ORDER BY
                    market_cap_usd DESC NULLS LAST,
                    COALESCE(t.symbol, dt.symbol, qt.symbol) ASC NULLS LAST,
                    tp.token_id ASC
                LIMIT $3
                OFFSET $4
                "#,
            )
            .bind(account_id)
            .bind(&query.q)
            .bind(fetch_n)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to search tokens: {}", e))?;

        let has_more = rows.len() as i64 > limit;
        let tokens: Vec<DexTokenEntry> = rows
            .into_iter()
            .take(limit as usize)
            .map(row_to_entry)
            .collect();
        let next_offset = if has_more { Some(offset + limit) } else { None };

        Ok(DexTokenListResponse {
            tokens,
            next_offset,
        })
    }
}

fn row_to_entry(r: SearchRow) -> DexTokenEntry {
    DexTokenEntry {
        token_id: r.token_id,
        symbol: r.symbol.unwrap_or_default(),
        name: r.name.unwrap_or_default(),
        decimals: r.decimals.unwrap_or(18),
        image_uri: r.image_uri.unwrap_or_default(),
        // Session-authed → balance always Some. "0" when no balance row.
        balance: Some(
            r.balance
                .unwrap_or_else(|| BigDecimal::from(0))
                .normalized()
                .to_plain_string(),
        ),
        market_cap_usd: r.market_cap_usd.map(|v| v.normalized().to_plain_string()),
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

    async fn seed_pool_with_two_tokens(pool: &PgPool) {
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

        // TOKEN0 needs creator FK to account
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

        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, 'WMON', 'WMON', 18, '', 0)"#,
        )
        .bind(TOKEN1)
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_controller(pool: PgPool) -> SearchController {
        use crate::db::postgres::PostgresDatabase;
        SearchController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_by_symbol_returns_match(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .search_tokens(
                &DexSearchQuery {
                    q: "CHO".to_string(),
                    limit: None,
                    offset: None,
                },
                ACCOUNT,
            )
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].symbol, "CHOG");
        // session-authed → balance always Some, "0" when no balance row
        assert_eq!(resp.tokens[0].balance.as_deref(), Some("0"));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_by_token_id_case_insensitive(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .search_tokens(
                &DexSearchQuery {
                    q: "bb01".to_string(),
                    limit: None,
                    offset: None,
                },
                ACCOUNT,
            )
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].token_id, TOKEN0);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_no_match_returns_empty(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .search_tokens(
                &DexSearchQuery {
                    q: "NONEXISTENT".to_string(),
                    limit: None,
                    offset: None,
                },
                ACCOUNT,
            )
            .await
            .unwrap();
        assert!(resp.tokens.is_empty());
        assert!(resp.next_offset.is_none());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_balance_attached_when_present(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        sqlx::query(
            r#"INSERT INTO balance (account_id, token_id, balance, created_at)
               VALUES ($1, $2, $3::NUMERIC, 0)"#,
        )
        .bind(ACCOUNT)
        .bind(TOKEN0)
        .bind("1000000000000000000000")
        .execute(&pool)
        .await
        .unwrap();
        let controller = make_controller(pool);
        let resp = controller
            .search_tokens(
                &DexSearchQuery {
                    q: "CHOG".to_string(),
                    limit: None,
                    offset: None,
                },
                ACCOUNT,
            )
            .await
            .unwrap();
        let chog = resp.tokens.iter().find(|t| t.symbol == "CHOG").unwrap();
        assert_eq!(chog.balance.as_deref(), Some("1000000000000000000000"));
    }
}
