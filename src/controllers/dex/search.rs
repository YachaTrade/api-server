use std::sync::Arc;

use anyhow::Result;

use crate::{
    controllers::dex::tokens::TokensController,
    db::postgres::PostgresDatabase,
    types::dex::{
        search::DexSearchQuery,
        tokens::{DexTokenListQuery, DexTokenListResponse},
    },
};

/// DEPRECATED. `/dex/search` now delegates to the unified `/dex/tokens` logic.
/// Kept for one release while the FE migrates to `GET /dex/tokens?q=`.
pub struct SearchController {
    db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Session-authenticated search. Delegates to `TokensController::list_tokens`
    /// with the session address as `account` and the search term as `q`.
    pub async fn search_tokens(
        &self,
        query: &DexSearchQuery,
        account_id: &str,
    ) -> Result<DexTokenListResponse> {
        let q = crate::utils::normalize_ca_query(&query.q);

        TokensController::new(self.db.clone())
            .list_tokens(&DexTokenListQuery {
                account: Some(account_id.to_string()),
                q: Some(q),
                page: query.page,
                limit: query.limit,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000aA11";
    const TOKEN0: &str = "0x000000000000000000000000000000000000bB01"; // CHOG
    const EXT: &str = "0x000000000000000000000000000000000000bB02"; // external

    fn make_controller(pool: PgPool) -> SearchController {
        SearchController::new(std::sync::Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_nadfun_v2_in_pool(pool: &PgPool) {
        // account (creator FK), token (nadfun V2), and a pool referencing it.
        sqlx::query(r#"INSERT INTO account (account_id, nickname, bio, image_uri, follower_count, following_count)
                       VALUES ($1,'c','','',0,0) ON CONFLICT DO NOTHING"#)
            .bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id, name, symbol, image_uri, creator, description,
                                          twitter, telegram, website, created_at, transaction_hash, total_supply, version)
                       VALUES ($1,'Chog Token','CHOG','',$2,'','','','',0,'0x',1000000000000000000000000,'V2')"#)
            .bind(TOKEN0).bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO pool (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                                         latest_trade_at, created_at, block_number, tx_hash)
                       VALUES ('0x9aEB5e0c5C8a3Bf3D6F8e8B7c3C2A1d0E9F8a7B6',$1,'0x0000000000000000000000000000000000000001',
                               100,100,1,0,0,0,0,1,'0x')"#)
            .bind(TOKEN0).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delegates_text_search_to_tokens(pool: PgPool) {
        seed_nadfun_v2_in_pool(&pool).await;
        sqlx::query("INSERT INTO balance (account_id, token_id, balance, created_at) VALUES ($1,$2,$3::NUMERIC,0)")
            .bind(ACCOUNT).bind(TOKEN0).bind("1000000000000000000000").execute(&pool).await.unwrap();
        let controller = make_controller(pool);
        let resp = controller.search_tokens(
            &DexSearchQuery { q: "CHO".into(), page: 1, limit: 50 }, ACCOUNT,
        ).await.unwrap();
        let chog = resp.tokens.iter().find(|t| t.token_id == TOKEN0).expect("CHOG present");
        assert!(chog.is_held, "session account → balance attached");
        assert_eq!(chog.balance.as_deref(), Some("1000000000000000000000"));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delegates_full_ca_exposes_external(pool: PgPool) {
        sqlx::query(r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
                       VALUES ($1,'Ext','EXT',18,'',0)"#).bind(EXT).execute(&pool).await.unwrap();
        let controller = make_controller(pool);
        let resp = controller.search_tokens(
            &DexSearchQuery { q: EXT.into(), page: 1, limit: 50 }, ACCOUNT,
        ).await.unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert!(resp.tokens[0].is_external);
    }
}
