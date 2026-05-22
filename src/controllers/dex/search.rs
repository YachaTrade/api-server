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
    symbol: String,
    name: String,
    decimals: i32,
    image_uri: String,
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
    ///
    /// Searches across `dex_token` — the canonical V2-tradeable token registry
    /// (populated by the indexer on every PairCreated). ILIKE branches reference
    /// `dex_token` columns directly so the planner can pick the per-column GIN
    /// trgm indexes from `0029_dex_search_indexes.sql`.
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
                SELECT
                    dt.token_id,
                    dt.symbol,
                    dt.name,
                    dt.decimals,
                    dt.image_uri,
                    b.balance                              AS balance,
                    (m.price * t.total_supply * lp.price)  AS market_cap_usd
                FROM dex_token dt
                LEFT JOIN balance b
                    ON b.token_id = dt.token_id
                   AND b.account_id = $1
                LEFT JOIN token t
                    ON t.token_id = dt.token_id
                LEFT JOIN market m
                    ON m.token_id = dt.token_id
                LEFT JOIN LATERAL (
                    SELECT price FROM price
                     WHERE quote_id = m.quote_id
                     ORDER BY block_number DESC
                     LIMIT 1
                ) lp ON true
                WHERE
                    -- Direct column refs (NOT COALESCE wrappers) so the
                    -- planner can use the per-column GIN trgm indexes from
                    -- 0029_dex_search_indexes.sql.
                    dt.symbol   ILIKE '%' || $2 || '%'
                 OR dt.name     ILIKE '%' || $2 || '%'
                 OR dt.token_id ILIKE '%' || $2 || '%'
                ORDER BY
                    market_cap_usd DESC NULLS LAST,
                    dt.symbol ASC,
                    dt.token_id ASC
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
        symbol: r.symbol,
        name: r.name,
        decimals: r.decimals,
        image_uri: r.image_uri,
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
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01"; // CHOG
    const TOKEN1: &str = "0x000000000000000000000000000000000000bB02"; // WMON

    async fn seed_dex_tokens(pool: &PgPool) {
        // Two V2-tradeable tokens. The /dex/search endpoint reads only
        // dex_token; pool / token / quote_token rows are not required for
        // search-result inclusion.
        for (id, sym, name) in [
            (TOKEN0, "CHOG", "Chog Token"),
            (TOKEN1, "WMON", "Wrapped Monad"),
        ] {
            sqlx::query(
                r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
                   VALUES ($1, $2, $3, 18, '', 0)"#,
            )
            .bind(id)
            .bind(name)
            .bind(sym)
            .execute(pool)
            .await
            .unwrap();
        }
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
        seed_dex_tokens(&pool).await;
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
    async fn search_by_name_returns_match(pool: PgPool) {
        seed_dex_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .search_tokens(
                &DexSearchQuery {
                    q: "Monad".to_string(),
                    limit: None,
                    offset: None,
                },
                ACCOUNT,
            )
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].symbol, "WMON");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_by_token_id_case_insensitive(pool: PgPool) {
        seed_dex_tokens(&pool).await;
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
        seed_dex_tokens(&pool).await;
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
        seed_dex_tokens(&pool).await;
        // account FK on balance: balance table has no FK to account, so this insert is safe.
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
