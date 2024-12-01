use anyhow::Result;

use rayon::iter::IntoParallelIterator;
use tracing::info;

use std::sync::Arc;

use crate::{
    db::postgres::{
        model::{Account, Thread, Token},
        PostgresDatabase,
    },
    types::response::{HoldTokenResponse, Identifier},
};

pub struct ProfileController {
    pub db: Arc<PostgresDatabase>,
}

impl ProfileController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ProfileController { db }
    }
    pub async fn get_profile(&self, identifier: &Identifier) -> Result<Account> {
        info!("identifier: {:?}", identifier);
        let account = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    Account,
                    "SELECT * FROM account WHERE nickname = $1",
                    nickname
                )
                .fetch_one(self.db.get_read_pool())
                .await?
            }
            Identifier::Address(address) => {
                sqlx::query_as!(
                    Account,
                    "SELECT * FROM account WHERE account_id = $1",
                    address
                )
                .fetch_one(self.db.get_read_pool())
                .await?
            }
        };

        Ok(account)
    }

    pub async fn get_holding_token(
        &self,
        identifier: &Identifier,
    ) -> Result<Vec<HoldTokenResponse>> {
        let holdings = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    HoldTokenResponse,
                    r#"
                    SELECT 
                        b.token_id,
                        COALESCE(b.current_amount::text, '0') as amount,
                        t.image_uri
                    FROM 
                        balance b
                    JOIN 
                        account a ON b.account_id = a.account_id
                    LEFT JOIN 
                        token t ON b.token_id = t.token_id
                    WHERE 
                        a.nickname = $1
                    ORDER BY 
                        b.current_amount DESC
                    LIMIT 50
                    "#,
                    nickname
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
            Identifier::Address(address) => {
                sqlx::query_as!(
                    HoldTokenResponse,
                    r#"
                    SELECT 
                        b.token_id,
                        COALESCE(b.current_amount::text, '0') as amount,
                        t.image_uri
                    FROM 
                        balance b
                    LEFT JOIN 
                        token t ON b.token_id = t.token_id
                    WHERE 
                        b.account_id = $1
                    ORDER BY 
                        b.current_amount DESC
                    LIMIT 50
                    "#,
                    address
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
        };

        Ok(holdings)
    }

    pub async fn get_replies(&self, identifier: &Identifier) -> Result<Vec<Thread>> {
        let replies = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    Thread,
                    r#"
                    SELECT t.*
                    FROM thread t
                    JOIN account a ON t.author_id = a.account_id
                    WHERE a.nickname = $1
                    ORDER BY t.created_at DESC
                    LIMIT 50
                    "#,
                    nickname
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
            Identifier::Address(address) => {
                sqlx::query_as!(
                    Thread,
                    r#"
                    SELECT *
                    FROM thread
                    WHERE author_id = $1
                    ORDER BY created_at DESC
                    LIMIT 50
                    "#,
                    address
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
        };

        Ok(replies)
    }

    pub async fn get_created_tokens(&self, identifier: &Identifier) -> Result<Vec<Token>> {
        let tokens = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    Token,
                    r#"
                    SELECT t.*
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    WHERE a.nickname = $1
                    ORDER BY t.created_at DESC
                    LIMIT 50
                    "#,
                    nickname
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
            Identifier::Address(address) => {
                sqlx::query_as!(
                    Token,
                    r#"
                    SELECT *
                    FROM token
                    WHERE creator = $1
                    ORDER BY created_at DESC
                    LIMIT 50
                    "#,
                    address
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
        };

        Ok(tokens)
    }

    pub async fn get_followers(&self, identifier: &Identifier) -> Result<Vec<Account>> {
        let followers = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    Account,
                    r#"
                    SELECT a2.account_id, a2.nickname, a2.bio, a2.image_uri, a2.follower_count, a2.following_count, a2.like_count
                    FROM follow f
                    JOIN account a1 ON f.following_id = a1.account_id
                    JOIN account a2 ON f.follower_id = a2.account_id
                    WHERE a1.nickname = $1
                    LIMIT 50
                    "#,
                    nickname
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            },
            Identifier::Address(address) => {
                sqlx::query_as!(
                    Account,
                    r#"
                    SELECT a.account_id, a.nickname, a.bio, a.image_uri, a.follower_count, a.following_count, a.like_count
                    FROM follow f
                    JOIN account a ON f.follower_id = a.account_id
                    WHERE f.following_id = $1
                    LIMIT 50
                    "#,
                    address
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
        };

        Ok(followers)
    }

    pub async fn get_following(&self, identifier: &Identifier) -> Result<Vec<Account>> {
        let followings = match identifier {
            Identifier::Nickname(nickname) => {
                sqlx::query_as!(
                    Account,
                    r#"
                    SELECT a2.account_id, a2.nickname, a2.bio, a2.image_uri, a2.follower_count, a2.following_count, a2.like_count
                    FROM follow f
                    JOIN account a1 ON f.follower_id = a1.account_id
                    JOIN account a2 ON f.following_id = a2.account_id
                    WHERE a1.nickname = $1
                    LIMIT 50
                    "#,
                    nickname
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            },
            Identifier::Address(address) => {
                sqlx::query_as!(
                    Account,
                    r#"
                    SELECT a.account_id, a.nickname, a.bio, a.image_uri, a.follower_count, a.following_count, a.like_count
                    FROM follow f
                    JOIN account a ON f.following_id = a.account_id
                    WHERE f.follower_id = $1
                    LIMIT 50
                    "#,
                    address
                )
                .fetch_all(self.db.get_read_pool())
                .await?
            }
        };

        Ok(followings)
    }
}
