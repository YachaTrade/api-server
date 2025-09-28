use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
    },
    types::trading::position::{HoldToken, HoldTokenResponse, TokenHolder, TokenHolderResponse},
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
            current_token_amount: BigDecimal,
            account_id: String,
            nickname: String,
            image_uri: String,
            follower_count: i32,
            following_count: i32,
            x_handle: Option<String>,
            x_image_uri: Option<String>,
            is_blue_label: Option<bool>,
        }

        let records = measure_postgres!(
            "position.fetch_holders_by_token",
            sqlx::query_as::<_, TokenHolderRow>(
                r#"
                SELECT 
                    b.balance as current_token_amount,
                    a.account_id,
                    a.nickname,
                    a.image_uri,
                    a.follower_count,
                    a.following_count,
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label
                FROM balance b
                JOIN account a ON b.account_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
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

        #[derive(sqlx::FromRow)]
        struct CreatorRow {
            creator: String,
        }

        let token_creator = measure_postgres!(
            "position.fetch_token_creator",
            sqlx::query_as::<_, CreatorRow>(
                r#"
                SELECT 
                    t.creator
                FROM token t
                WHERE t.token_id = $1   
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token creator: {}", err))?;

        let token_creator = token_creator.creator;

        let holders = records
            .into_iter()
            .map(|row| TokenHolder {
                current_amount: row.current_token_amount.to_plain_string(),
                is_dev: row.account_id == token_creator,
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row
                        .x_handle
                        .clone()
                        .filter(|handle| !handle.is_empty())
                        .unwrap_or(row.nickname),
                    image_uri: row
                        .x_image_uri
                        .clone()
                        .filter(|img| !img.is_empty())
                        .unwrap_or(row.image_uri),
                    follower_count: row.follower_count,
                    following_count: row.following_count,
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
            balance: BigDecimal,
            price: BigDecimal,
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
                    b.balance,
                    m.price
                FROM token t
                JOIN balance b ON t.token_id = b.token_id
                JOIN market m ON t.token_id = m.token_id
                WHERE b.account_id = $1 AND b.balance > 0
                ORDER BY (b.balance * m.price) DESC
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
            .map(|row| HoldToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                balance: row.balance.to_string(),
                value: (row.balance * row.price).to_string(),
            })
            .collect();

        Ok(HoldTokenResponse {
            tokens,
            total_count,
        })
    }
}
