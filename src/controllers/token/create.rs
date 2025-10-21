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
            info::{AccountInfo, BalanceInfo, MarketInfo, MarketType, TokenCreatedInfo, TokenInfo},
            pagination::PaginationParams,
        },
        profile::CreatedTokensResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct TokenCreatedController {
    db: Arc<PostgresDatabase>,
}

impl TokenCreatedController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenCreatedController { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let cache_key = cache_key!("token_created_count", account_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = TokenCreatedController::new(db);
                controller.fetch_total_count(&account_id).await
            }
        })
        .await?;

        Ok(count)
    }

    async fn fetch_total_count(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "token_created.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM token t
                WHERE t.creator = $1
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token created count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<CreatedTokensResponse> {
        let cache_key = cache_key!(
            "tokens_created",
            account_id,
            pagination.page,
            pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = PaginationParams {
                page: pagination.page,
                limit: pagination.limit,
                direction: pagination.direction.clone(),
            };
            async move {
                let controller = TokenCreatedController::new(db);
                controller
                    .fetch_tokens_created(&account_id, &pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<CreatedTokensResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct TokenCreatedRow {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image_uri: String,
            token_description: Option<String>,
            token_twitter: Option<String>,
            token_telegram: Option<String>,
            token_website: Option<String>,
            is_listing: bool,
            token_created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            creator_follower_count: i32,
            creator_following_count: i32,
            market_type: String,
            market_id: String,
            token_price: BigDecimal,
            native_price: BigDecimal,
            price: BigDecimal,
            total_supply: BigDecimal,
            liquidity: BigDecimal,
            volume: BigDecimal,
            balance: BigDecimal,
            balance_created_at: i64,
        }

        let tokens = measure_postgres!(
            "token_created.fetch_tokens_created",
            sqlx::query_as::<_, TokenCreatedRow>(
                r#"
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                created_tokens AS (
                    SELECT
                        t.token_id,
                        t.name as token_name,
                        t.symbol as token_symbol,
                        t.image_uri as token_image_uri,
                        t.description as token_description,
                        t.twitter as token_twitter,
                        t.telegram as token_telegram,
                        t.website as token_website,
                        t.is_listing,
                        t.created_at as token_created_at,
                        t.creator,
                        m.token_holder_count as holder_count,
                        COALESCE(
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                            a.nickname
                        ) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        t.total_supply,
                        COALESCE(m.reserve_native, 0) as liquidity,
                        m.volume,
                        COALESCE(b.balance, 0) as balance,
                        COALESCE(b.created_at, 0) as balance_created_at,
                        COALESCE(m.price * b.balance * COALESCE(lp.price, 0), 0) as current_value
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                    CROSS JOIN latest_price lp
                    WHERE t.creator = $1
                )
                SELECT
                    token_id,
                    token_name,
                    token_symbol,
                    token_image_uri,
                    token_description,
                    token_twitter,
                    token_telegram,
                    token_website,
                    is_listing,
                    token_created_at,
                    creator,
                    creator_nickname,
                    creator_bio,
                    creator_image_uri,
                    creator_follower_count,
                    creator_following_count,
                    market_type,
                    market_id,
                    token_price,
                    native_price,
                    price,
                    total_supply,
                    balance
                FROM created_tokens
                ORDER BY current_value DESC
                LIMIT $2
                OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit as i64)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch tokens created: {}", err))?;

        let tokens: Vec<TokenCreatedInfo> = tokens
            .into_iter()
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if row.market_type == "CURVE" && market_id.is_empty() {
                    market_id = BONDING_CURVE.clone();
                }

                TokenCreatedInfo {
                    token_info: TokenInfo {
                        token_id: row.token_id.clone(),
                        name: row.token_name,
                        symbol: row.token_symbol,
                        image_uri: row.token_image_uri,
                        description: row.token_description,
                        is_listing: row.is_listing,
                        twitter: row.token_twitter,
                        telegram: row.token_telegram,
                        website: row.token_website,
                        created_at: row.token_created_at,
                        creator: AccountInfo {
                            account_id: row.creator,
                            nickname: row.creator_nickname,
                            bio: row.creator_bio,
                            image_uri: row.creator_image_uri,
                            follower_count: row.creator_follower_count,
                            following_count: row.creator_following_count,
                        },
                    },
                    market_info: MarketInfo {
                        market_type: match row.market_type.as_str() {
                            "CURVE" => MarketType::Curve,
                            "DEX" => MarketType::Dex,
                            _ => MarketType::Curve,
                        },
                        token_id: row.token_id,
                        market_id,
                        token_price: row.token_price.to_plain_string(),
                        native_price: row.native_price.to_plain_string(),
                        price: row.price.to_plain_string(),
                        total_supply: row.total_supply.to_plain_string(),
                        liquidity: row.liquidity.to_plain_string(),
                        volume: row.volume.to_plain_string(),
                        holder_count: row.holder_count,
                    },
                    balance_info: BalanceInfo {
                        balance: row.balance.to_plain_string(),
                        token_price: row.token_price.to_plain_string(),
                        native_price: row.native_price.to_plain_string(),
                        created_at: row.balance_created_at,
                    },
                }
            })
            .collect();

        let total_count = self.fetch_total_count(account_id).await?;

        Ok(CreatedTokensResponse {
            tokens,
            total_count,
        })
    }
}
