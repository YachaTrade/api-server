use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::tokens::{DexTokenEntry, DexTokenListQuery, DexTokenListResponse},
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    token_id: String,
    symbol: Option<String>,
    name: Option<String>,
    decimals: Option<i32>,
    image_uri: Option<String>,
    balance: Option<BigDecimal>,
    market_cap_usd: Option<BigDecimal>,
}

pub struct TokensController {
    db: Arc<PostgresDatabase>,
}

impl TokensController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn list_tokens(&self, query: &DexTokenListQuery) -> Result<DexTokenListResponse> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = query.offset.unwrap_or(0).max(0);

        // Fetch one extra row to detect "has more pages" cheaply.
        let fetch_n = limit + 1;

        let rows = measure_postgres!(
            "dex.list_tokens",
            sqlx::query_as::<_, TokenRow>(
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
                    (m.price * t.total_supply * lp.price)
                                                                      AS market_cap_usd
                FROM tokens_in_pools tp
                LEFT JOIN token       t   ON t.token_id  = tp.token_id
                LEFT JOIN dex_token   dt  ON dt.token_id = tp.token_id
                LEFT JOIN quote_token qt  ON qt.quote_id = tp.token_id
                LEFT JOIN balance     b   ON b.token_id  = tp.token_id
                                         AND b.account_id = COALESCE($1::VARCHAR, '__NO_ACCOUNT__')
                LEFT JOIN market      m   ON m.token_id  = tp.token_id
                LEFT JOIN LATERAL (
                    SELECT price FROM price
                     WHERE quote_id = m.quote_id
                     ORDER BY block_number DESC
                     LIMIT 1
                ) lp ON true
                WHERE
                    ($2::TEXT IS NULL
                     OR COALESCE(t.symbol,    dt.symbol,    qt.symbol)    ILIKE '%' || $2 || '%'
                     OR COALESCE(t.name,      dt.name,      qt.name)      ILIKE '%' || $2 || '%'
                     OR tp.token_id                                       ILIKE '%' || $2 || '%'
                    )
                ORDER BY
                    market_cap_usd DESC NULLS LAST,
                    COALESCE(t.symbol, dt.symbol, qt.symbol) ASC NULLS LAST,
                    tp.token_id ASC
                LIMIT $3
                OFFSET $4
                "#,
            )
            .bind(query.account.as_deref())
            .bind(query.q.as_deref())
            .bind(fetch_n)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|e| anyhow::anyhow!("Failed to list tokens: {}", e))?;

        // "has more" detection: we asked for limit+1; if we got more than `limit`, next page exists.
        let has_more = rows.len() as i64 > limit;
        let tokens: Vec<DexTokenEntry> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| row_to_entry(r, query.account.is_some()))
            .collect();

        let next_offset = if has_more { Some(offset + limit) } else { None };

        Ok(DexTokenListResponse { tokens, next_offset })
    }
}

fn row_to_entry(r: TokenRow, account_provided: bool) -> DexTokenEntry {
    DexTokenEntry {
        token_id: r.token_id,
        symbol: r.symbol.unwrap_or_default(),
        name: r.name.unwrap_or_default(),
        decimals: r.decimals.unwrap_or(18),
        image_uri: r.image_uri.unwrap_or_default(),
        // When ?account= was provided, always return Some — zero balance and
        // missing-row both render as "0" so FE can distinguish "queried, no
        // holdings" from "no account passed".
        balance: if account_provided {
            Some(
                r.balance
                    .unwrap_or_else(|| BigDecimal::from(0))
                    .normalized()
                    .to_plain_string(),
            )
        } else {
            None
        },
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

        // TOKEN1: pure dex_token (no `token` row → marketcap NULL).
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

    fn make_controller(pool: PgPool) -> TokensController {
        use crate::db::postgres::PostgresDatabase;
        TokensController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    fn empty_query() -> DexTokenListQuery {
        DexTokenListQuery {
            account: None,
            q: None,
            limit: None,
            offset: None,
        }
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn lists_pool_backed_tokens_no_filter(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        assert_eq!(resp.tokens.len(), 2);
        assert!(resp.next_offset.is_none(), "only 2 tokens → no next page");

        let chog = resp
            .tokens
            .iter()
            .find(|t| t.symbol == "CHOG")
            .expect("CHOG present");
        assert_eq!(chog.token_id, TOKEN0);
        assert!(chog.balance.is_none(), "no ?account= → balance is None");

        let wmon = resp
            .tokens
            .iter()
            .find(|t| t.symbol == "WMON")
            .expect("WMON present");
        assert_eq!(wmon.token_id, TOKEN1);
        assert!(
            wmon.market_cap_usd.is_none(),
            "dex_token-only → marketcap None"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn includes_balance_when_account_provided(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        seed_balance(&pool, TOKEN0, "1000000000000000000000").await; // 1000 CHOG (18 dp)
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: Some(ACCOUNT.to_string()),
                q: None,
                limit: None,
                offset: None,
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

        let wmon = resp
            .tokens
            .iter()
            .find(|t| t.symbol == "WMON")
            .expect("WMON present");
        assert_eq!(
            wmon.balance.as_deref(),
            Some("0"),
            "no balance row but account provided → Some(\"0\")"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_filters_by_symbol(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("CHO".to_string()),
                limit: None,
                offset: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].symbol, "CHOG");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn search_filters_by_token_id_prefix(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: Some("bb01".to_string()), // lowercase, ILIKE handles case
                limit: None,
                offset: None,
            })
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].token_id, TOKEN0);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pagination_next_offset_present_when_more_rows(pool: PgPool) {
        seed_pool_with_two_tokens(&pool).await;
        let controller = make_controller(pool);
        let resp = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: None,
                limit: Some(1),
                offset: Some(0),
            })
            .await
            .unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.next_offset, Some(1), "1 page consumed, next page exists");

        let resp2 = controller
            .list_tokens(&DexTokenListQuery {
                account: None,
                q: None,
                limit: Some(1),
                offset: Some(1),
            })
            .await
            .unwrap();
        assert_eq!(resp2.tokens.len(), 1);
        assert!(resp2.next_offset.is_none(), "consumed last row → no more pages");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn returns_empty_when_no_pools(pool: PgPool) {
        let controller = make_controller(pool);
        let resp = controller.list_tokens(&empty_query()).await.unwrap();
        assert!(resp.tokens.is_empty());
        assert!(resp.next_offset.is_none());
    }
}
