use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::types::BigDecimal;

use crate::{
    cache_key,
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, MarketInfo, MarketType, TokenInfo, TokenVersion},
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
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
    version: TokenVersion,
            created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_image_uri: String,
            creator_bio: String,
            market_type: String,
            market_id: String,
    quote_id: String,
            token_price: BigDecimal,
            native_price: BigDecimal,
            price: BigDecimal,
            price_usd: BigDecimal,
            total_supply: BigDecimal,
            reserve_native: BigDecimal,
            reserve_token: BigDecimal,
            ath_price: BigDecimal,
            ath_price_native: BigDecimal,
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
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                        t.version,
                    t.created_at,
                    t.creator,
                    t.token_holder_count as holder_count,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.bio as creator_bio,
                    m.market_type,
                    COALESCE(m.pool_id, '') as market_id,
                    m.quote_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    m.price,
                    (m.price * COALESCE(lp.price, 0)) as price_usd,
                    t.total_supply,
                    COALESCE(m.reserve_native, 0) as reserve_native,
                    COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume,
                    m.ath_price,
                    m.ath_price_native
                FROM token t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                CROSS JOIN latest_price lp
                WHERE t.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token metadata: {}", err))?;

        let token_info = TokenInfo {
            token_id: row.token_id.clone(),
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description,
            is_graduated: row.is_graduated,
            is_nsfw: row.is_nsfw,
            twitter: row.twitter,
            telegram: row.telegram,
            website: row.website,
            created_at: row.created_at,
            creator: AccountInfo {
                account_id: row.creator,
                nickname: row.creator_nickname,
                bio: row.creator_bio,
                image_uri: row.creator_image_uri,
            },
            is_cto: row.is_cto,
                    version: row.version.clone(),
            hackathon_info: None,
        };

        let mut market_id = row.market_id;
        if market_id.is_empty() {
            if row.market_type == "CURVE" { market_id = V1_BONDING_CURVE.clone(); } else if row.market_type == "V2_CURVE" { market_id = V2_BONDING_CURVE.clone(); }
        }

        let market_info = MarketInfo {
            market_type: match row.market_type.as_str() {
                "CURVE" => MarketType::Curve,
                "DEX" => MarketType::Dex,
                    "V2_CURVE" => MarketType::V2Curve,
                    "V2_DEX" => MarketType::V2Dex,
                _ => MarketType::Curve,
            },
            token_id: row.token_id,
                    quote_id: row.quote_id.clone(),
            market_id,
            token_price: row.token_price.normalized().to_plain_string(),
            native_price: row.native_price.normalized().to_plain_string(),
            price: row.price.normalized().to_plain_string(),
            price_usd: row.price_usd.normalized().to_plain_string(),
            price_native: row.price.normalized().to_plain_string(),
            total_supply: row.total_supply.normalized().to_plain_string(),
            reserve_native: row.reserve_native.normalized().to_plain_string(),
            reserve_token: row.reserve_token.normalized().to_plain_string(),
            volume: row.volume.normalized().to_plain_string(),
            ath_price: row.ath_price.normalized().to_plain_string(),
            ath_price_usd: row.ath_price.normalized().to_plain_string(),
            ath_price_native: row.ath_price_native.normalized().to_plain_string(),
            holder_count: row.holder_count,
        };

        Ok(TokenMetadataResponse {
            token_info,
            market_info,
        })
    }
}
