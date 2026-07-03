pub mod create;
pub mod gift_fee;
pub mod metadata;
pub mod order;

use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, TokenInfo, TokenVersion},
        token::{
            TokenResponse,
            x_verification::{TokenXVerification, XFollowedByEntry},
        },
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    token_id: String,
    name: String,
    symbol: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    image_uri: String,
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_image_uri: String,
    creator_bio: String,
    // NULL unless this token has a token_x_verification row.
    x_followers_count: Option<i64>,
}

pub struct TokenController {
    db: Arc<PostgresDatabase>,
}

impl TokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenController { db }
    }

    pub async fn get_token(&self, token_id: &str) -> Result<TokenResponse> {
        let cache_key = cache_key!("token", token_id);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_token(token_id).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_token(&self, token_id: &str) -> Result<TokenResponse> {
        let row = measure_postgres!(
            "token.fetch_token",
            sqlx::query_as::<_, TokenRow>(
                r#"
                SELECT
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.description,
                    t.twitter,
                    t.telegram,
                    t.website,
                    t.image_uri,
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                        t.version,
                    t.created_at,
                    t.creator,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.bio as creator_bio,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count,
                    txv.followers_count as x_followers_count
                FROM token t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN token_x_verification txv ON txv.token_id = t.token_id
                WHERE t.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token: {}", err))?;

        // Only load followed_by when a verification row exists.
        let x_verification = match row.x_followers_count {
            Some(followers_count) => {
                #[derive(sqlx::FromRow)]
                struct FollowedByRow {
                    x_handle: String,
                    x_image_uri: String,
                    x_followers_count: i64,
                    is_x_verified: bool,
                }
                let rows = measure_postgres!(
                    "token.fetch_followed_by",
                    sqlx::query_as::<_, FollowedByRow>(
                        r#"
                        SELECT x_handle, x_image_uri, x_followers_count, is_x_verified
                        FROM token_x_followed_by
                        WHERE token_id = $1
                        ORDER BY checked_at ASC
                        "#,
                    )
                    .bind(token_id)
                    .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to get followed_by: {}", err))?;

                Some(TokenXVerification {
                    followers_count,
                    followed_by: rows
                        .into_iter()
                        .map(|r| XFollowedByEntry {
                            x_handle: r.x_handle,
                            x_image_uri: r.x_image_uri,
                            x_followers_count: r.x_followers_count,
                            is_x_verified: r.is_x_verified,
                        })
                        .collect(),
                })
            }
            None => None,
        };

        let token_info = TokenInfo {
            token_id: row.token_id,
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description,
            is_graduated: row.is_graduated,
            is_nsfw: row.is_nsfw,
            twitter: row.twitter.filter(|s| !s.is_empty()),
            telegram: row.telegram.filter(|s| !s.is_empty()),
            website: row.website.filter(|s| !s.is_empty()),
            created_at: row.created_at,
            creator: AccountInfo {
                account_id: row.creator,
                nickname: row.creator_nickname,
                bio: row.creator_bio,
                image_uri: row.creator_image_uri,
            },
            is_cto: row.is_cto,
            version: row.version.clone(),
            x_verification,
        };

        Ok(TokenResponse { token_info })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const TOKEN: &str = "0x000000000000000000000000000000000000B143";
    const CREATOR: &str = "0x000000000000000000000000000000000000Aa01";
    // Distinct from TOKEN/CREATOR above: `get_token` routes through the
    // process-wide GLOBAL_CACHE (keyed only by token_id, 1s TTL), which is
    // shared across tests in this binary even though each #[sqlx::test]
    // gets its own isolated database. Reusing TOKEN here would let whichever
    // test runs first poison the other's cached result.
    const TOKEN_VERIFIED: &str = "0x000000000000000000000000000000000000B144";
    const CREATOR_VERIFIED: &str = "0x000000000000000000000000000000000000Aa02";

    fn ctrl(pool: PgPool) -> TokenController {
        TokenController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_token(pool: &PgPool, token: &str, creator: &str) {
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'c','','') ON CONFLICT DO NOTHING")
            .bind(creator).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version)
            VALUES ($1,'T','T','',$2,NULL,false,false,false,1,'0xh',1000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(token).bind(creator).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn unverified_token_has_none(pool: PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        let resp = ctrl(pool).get_token(TOKEN).await.unwrap();
        assert!(resp.token_info.x_verification.is_none());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn verified_token_returns_nested_signals(pool: PgPool) {
        seed_token(&pool, TOKEN_VERIFIED, CREATOR_VERIFIED).await;
        sqlx::query("INSERT INTO token_x_verification (token_id,account_id,x_user_id,followers_count) VALUES ($1,$2,'9',128000)")
            .bind(TOKEN_VERIFIED).bind(CREATOR_VERIFIED).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO token_x_followed_by (token_id,x_handle,x_image_uri,x_followers_count,is_x_verified) VALUES ($1,'elonmusk','https://img',200000000,true)")
            .bind(TOKEN_VERIFIED).execute(&pool).await.unwrap();

        let resp = ctrl(pool).get_token(TOKEN_VERIFIED).await.unwrap();
        let xv = resp.token_info.x_verification.expect("verified");
        assert_eq!(xv.followers_count, 128000);
        assert_eq!(xv.followed_by.len(), 1);
        assert_eq!(xv.followed_by[0].x_handle, "elonmusk");
        assert_eq!(xv.followed_by[0].is_x_verified, true);
    }
}
