use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::types::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, MarketInfo, MarketType, TokenInfo},
        token::metadata::TokenMetadataResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct TokenMetadataController {
    db: Arc<PostgresDatabase>,
}

impl TokenMetadataController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenMetadataController { db }
    }

    pub async fn get_token_metadata(&self, token_id: &str) -> Result<TokenMetadataResponse> {
        let cache_key = cache_key!("token_metadata", token_id);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let token_id = token_id.to_string();
            async move {
                let controller = TokenMetadataController::new(db);
                controller.fetch_token_metadata(&token_id).await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_token_metadata(&self, token_id: &str) -> Result<TokenMetadataResponse> {
        #[derive(sqlx::FromRow)]
        struct TokenMetadataRow {
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
            holder_count: i64,
            creator_nickname: String,
            creator_image_uri: String,
            creator_bio: String,
            creator_follower_count: i32,
            creator_following_count: i32,
            market_type: String,
            market_id: String,
            token_price: BigDecimal,
            native_price: BigDecimal,
            price: BigDecimal,
            total_supply: BigDecimal,
            liquidity: BigDecimal,
            ath_price: BigDecimal,
            volume: BigDecimal,
        }

        let row = measure_postgres!(
            "token.fetch_token_metadata",
            sqlx::query_as::<_, TokenMetadataRow>(
                r#"
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                )
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
                    t.token_holder_count as holder_count,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as creator_nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.bio as creator_bio,
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
                    m.ath_price
                FROM token t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                JOIN market m ON t.token_id = m.token_id
                CROSS JOIN latest_price lp
                WHERE t.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(&*self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token metadata: {}", err))?;

        let token_info = TokenInfo {
            token_id: row.token_id.clone(),
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
        };

        let mut market_id = row.market_id;
        if row.market_type == "CURVE" && market_id.is_empty() {
            market_id = BONDING_CURVE.clone();
        }

        let market_info = MarketInfo {
            market_type: match row.market_type.as_str() {
                "CURVE" => MarketType::Curve,
                "DEX" => MarketType::Dex,
                _ => MarketType::Curve,
            },
            token_id: row.token_id,
            market_id,
            token_price: row.token_price.normalized().to_plain_string(),
            native_price: row.native_price.normalized().to_plain_string(),
            price: row.price.normalized().to_plain_string(),
            total_supply: row.total_supply.normalized().to_plain_string(),
            liquidity: row.liquidity.normalized().to_plain_string(),
            volume: row.volume.normalized().to_plain_string(),
            ath_price: row.ath_price.normalized().to_plain_string(),
            holder_count: row.holder_count,
        };

        Ok(TokenMetadataResponse {
            token_info,
            market_info,
        })
    }
}
