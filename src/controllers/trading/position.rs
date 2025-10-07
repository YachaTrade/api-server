use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{AccountInfo, BalanceInfo, TokenInfo, TokenWithBalanceInfo},
        pagination::PaginationParams,
    },
    types::{
        profile::HoldTokenResponse,
        trading::position::{TokenHolder, TokenHolderResponse},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }

    pub async fn get_total_count_by_token_holder(&self, token_id: &str) -> Result<i64> {
        let cache_key = cache_key!("token_holder_count", token_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_token_holder_count(token_id).await
        })
        .await?;

        Ok(count)
    }

    async fn fetch_token_holder_count(&self, token_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "position.fetch_token_holder_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(holder_count, 0) as count
                FROM token_holder_count
                WHERE token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holder count: {}", err))?;

        Ok(count.map(|c| c.count).unwrap_or(0))
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let cache_key = cache_key!("token_holders", token_id, pagination.page, pagination.limit);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_holders_by_token(token_id, pagination).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct TokenHolderRow {
            balance: BigDecimal,
            token_price: BigDecimal,
            native_price: BigDecimal,
            account_id: String,
            nickname: String,
            bio: String,
            image_uri: String,
            follower_count: i32,
            following_count: i32,
        }

        let records = measure_postgres!(
            "position.fetch_holders_by_token",
            sqlx::query_as::<_, TokenHolderRow>(
                r#"
                SELECT
                    b.balance,
                    (m.price * COALESCE(p.price, 0)) as token_price,
                    COALESCE(p.price, 0) as native_price,
                    a.account_id,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as nickname,
                    a.bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    a.follower_count,
                    a.following_count
                FROM balance b
                JOIN account a ON b.account_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                JOIN market m ON b.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ) p ON true
                WHERE b.token_id = $1 AND b.balance > 0
                ORDER BY b.balance DESC
                OFFSET $2 LIMIT $3
                "#,
            )
            .bind(token_id)
            .bind(offset)
            .bind(pagination.limit as i64)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holders: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_token_holder(token_id).await?
        };

        let holders = records
            .into_iter()
            .map(|row| TokenHolder {
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.nickname,
                    bio: row.bio,
                    image_uri: row.image_uri,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                },
                balance_info: BalanceInfo {
                    balance: row.balance.to_plain_string(),
                    token_price: row.token_price.to_plain_string(),
                    native_price: row.native_price.to_plain_string(),
                },
            })
            .collect();

        Ok(TokenHolderResponse {
            holders,
            total_count,
        })
    }

    pub async fn get_total_count_by_hold_token(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_hold_token",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM balance b
                WHERE b.account_id = $1 AND b.balance > 0
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct HoldTokenRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            description: Option<String>,
            twitter: Option<String>,
            telegram: Option<String>,
            website: Option<String>,
            is_listing: bool,
            created_at: i64,
            creator: String,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            creator_follower_count: i32,
            creator_following_count: i32,
            balance: BigDecimal,
            token_price: BigDecimal,
            native_price: BigDecimal,
        }

        let records = measure_postgres!(
            "position.get_hold_token_by_account",
            sqlx::query_as::<_, HoldTokenRow>(
                r#"
                SELECT
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    t.description,
                    t.twitter,
                    t.telegram,
                    t.website,
                    t.is_listing,
                    t.created_at,
                    t.creator,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count,
                    b.balance,
                    (m.price * COALESCE(p.price, 0)) as token_price,
                    COALESCE(p.price, 0) as native_price
                FROM token t
                JOIN balance b ON t.token_id = b.token_id
                JOIN market m ON t.token_id = m.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                LEFT JOIN LATERAL (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ) p ON true
                WHERE b.account_id = $1 AND b.balance > 0
                ORDER BY (b.balance * m.price * COALESCE(p.price, 0)) DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token by account: {}", err))?;

        let total_count = if records.is_empty() {
            0
        } else {
            self.get_total_count_by_hold_token(account_id).await?
        };

        let tokens = records
            .into_iter()
            .map(|row| TokenWithBalanceInfo {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    description: row.description,
                    is_listing: row.is_listing,
                    twitter: row.twitter,
                    telegram: row.telegram,
                    website: row.website,
                    created_at: row.created_at,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                        follower_count: row.creator_follower_count,
                        following_count: row.creator_following_count,
                    },
                },
                balance_info: BalanceInfo {
                    balance: row.balance.to_string(),
                    token_price: row.token_price.to_string(),
                    native_price: row.native_price.to_string(),
                },
            })
            .collect();

        Ok(HoldTokenResponse {
            tokens,
            total_count,
        })
    }
}
