use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::info;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::info::{AccountInfo, TokenInfo},
    utils::single_flight::{with_cache, GLOBAL_CACHE},
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewContentResponse {
    pub new_buy: Option<NewSwapMessage>,
    pub new_sell: Option<NewSwapMessage>,
    pub new_token: Option<NewTokenMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewSwapMessage {
    pub account_info: AccountInfo,
    pub token_info: TokenInfo,
    pub is_buy: bool,
    pub amount: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewTokenMessage {
    pub account_info: AccountInfo,
    pub token_info: TokenInfo,
}

pub struct NewContentController {
    pub db: Arc<PostgresDatabase>,
}

impl NewContentController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        NewContentController { db }
    }

    pub async fn get_latest_new_buy(&self) -> Result<Option<NewSwapMessage>> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = "new_content:latest_buy";
        
        // Single Flight Pattern 적용
        let result = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = NewContentController::new(db);
                controller.fetch_latest_new_buy().await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!("get_latest_new_buy() completed in {:?}", elapsed);
        
        if elapsed > Duration::from_millis(100) {
            tracing::warn!("get_latest_new_buy() slow performance: {:?}", elapsed);
        }
        
        Ok(result)
    }
    
    async fn fetch_latest_new_buy(&self) -> Result<Option<NewSwapMessage>> {


        let query = r#"
            SELECT
                s.is_buy,
                s.native_amount,
                a.nickname,
                a.image_uri,
                a.follower_count,
                a.following_count,
                t.name as token_name,
                t.symbol as token_symbol,
                t.image_uri as token_image_uri,
                t.token_id as token_id,
                a.account_id as account_id,
                CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                ax.x_image_uri
            FROM swap s
            JOIN account a ON s.account_id = a.account_id
            JOIN token t ON s.token_id = t.token_id
            LEFT JOIN account_x ax ON s.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE s.is_buy = true
            ORDER BY s.created_at DESC
            LIMIT 1
        "#;

        let query_future = sqlx::query(query).fetch_optional(self.db.get_read_pool());

        let row_opt = tokio::time::timeout(Duration::from_millis(1000), query_future)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to get latest new buy: {}", err))?;

        let result = row_opt.map(|row| NewSwapMessage {
            account_info: AccountInfo {
                account_id: row.get("account_id"),
                nickname: if let Some(x_handle) = row.get::<Option<String>, _>("x_handle") {
                    if !x_handle.is_empty() {
                        x_handle
                    } else {
                        row.get("nickname")
                    }
                } else {
                    row.get("nickname")
                },
                image_uri: if let Some(x_image_uri) = row.get::<Option<String>, _>("x_image_uri") {
                    if !x_image_uri.is_empty() {
                        x_image_uri
                    } else {
                        row.get("image_uri")
                    }
                } else {
                    row.get("image_uri")
                },
                follower_count: row.get("follower_count"),
                following_count: row.get("following_count"),
            },
            token_info: TokenInfo {
                token_id: row.get("token_id"),
                name: row.get("token_name"),
                symbol: row.get("token_symbol"),
                image_uri: row.get("token_image_uri"),
            },
            is_buy: row.get("is_buy"),
            amount: row.get("native_amount"),
        });

        Ok(result)
    }

    pub async fn get_latest_new_sell(&self) -> Result<Option<NewSwapMessage>> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = "new_content:latest_sell";
        
        // Single Flight Pattern 적용
        let result = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = NewContentController::new(db);
                controller.fetch_latest_new_sell().await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!("get_latest_new_sell() completed in {:?}", elapsed);
        
        if elapsed > Duration::from_millis(100) {
            tracing::warn!("get_latest_new_sell() slow performance: {:?}", elapsed);
        }
        
        Ok(result)
    }
    
    async fn fetch_latest_new_sell(&self) -> Result<Option<NewSwapMessage>> {


        let query = r#"
            SELECT
                s.is_buy,
                s.native_amount,
                a.nickname,
                a.image_uri,
                a.follower_count,
                a.following_count,
                t.name as token_name,
                t.symbol as token_symbol,
                t.image_uri as token_image_uri,
                t.token_id as token_id,
                a.account_id as account_id,
                CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                ax.x_image_uri
            FROM swap s
            JOIN account a ON s.account_id = a.account_id
            JOIN token t ON s.token_id = t.token_id
            LEFT JOIN account_x ax ON s.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE s.is_buy = false
            ORDER BY s.created_at DESC
            LIMIT 1
        "#;

        let query_future = sqlx::query(query).fetch_optional(self.db.get_read_pool());

        let row_opt = tokio::time::timeout(Duration::from_millis(1000), query_future)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to get latest new sell: {}", err))?;

        let result = row_opt.map(|row| NewSwapMessage {
            account_info: AccountInfo {
                account_id: row.get("account_id"),
                nickname: if let Some(x_handle) = row.get::<Option<String>, _>("x_handle") {
                    if !x_handle.is_empty() {
                        x_handle
                    } else {
                        row.get("nickname")
                    }
                } else {
                    row.get("nickname")
                },
                image_uri: if let Some(x_image_uri) = row.get::<Option<String>, _>("x_image_uri") {
                    if !x_image_uri.is_empty() {
                        x_image_uri
                    } else {
                        row.get("image_uri")
                    }
                } else {
                    row.get("image_uri")
                },
                follower_count: row.get("follower_count"),
                following_count: row.get("following_count"),
            },
            token_info: TokenInfo {
                token_id: row.get("token_id"),
                name: row.get("token_name"),
                symbol: row.get("token_symbol"),
                image_uri: row.get("token_image_uri"),
            },
            is_buy: row.get("is_buy"),
            amount: row.get("native_amount"),
        });

        Ok(result)
    }

    pub async fn get_latest_new_token(&self) -> Result<Option<NewTokenMessage>> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = "new_content:latest_token";
        
        // Single Flight Pattern 적용
        let result = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = NewContentController::new(db);
                controller.fetch_latest_new_token().await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!("get_latest_new_token() completed in {:?}", elapsed);
        
        if elapsed > Duration::from_millis(100) {
            tracing::warn!("get_latest_new_token() slow performance: {:?}", elapsed);
        }
        
        Ok(result)
    }
    
    async fn fetch_latest_new_token(&self) -> Result<Option<NewTokenMessage>> {


        let query = r#"
            SELECT 
                t.name as token_name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.created_at,
                t.token_id as token_id,
                a.nickname,
                a.image_uri,
                a.account_id,
                a.follower_count,
                a.following_count,
                CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                ax.x_image_uri
            FROM token t
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON t.creator = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            ORDER BY t.created_at DESC
            LIMIT 1
        "#;

        let query_future = sqlx::query(query).fetch_optional(self.db.get_read_pool());

        let row_opt = tokio::time::timeout(Duration::from_millis(1000), query_future)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to get latest new token: {}", err))?;

        let result = row_opt.map(|row| NewTokenMessage {
            account_info: AccountInfo {
                account_id: row.get("account_id"),
                nickname: if let Some(x_handle) = row.get::<Option<String>, _>("x_handle") {
                    if !x_handle.is_empty() {
                        x_handle
                    } else {
                        row.get("nickname")
                    }
                } else {
                    row.get("nickname")
                },
                image_uri: if let Some(x_image_uri) = row.get::<Option<String>, _>("x_image_uri") {
                    if !x_image_uri.is_empty() {
                        x_image_uri
                    } else {
                        row.get("image_uri")
                    }
                } else {
                    row.get("image_uri")
                },
                follower_count: row.get("follower_count"),
                following_count: row.get("following_count"),
            },
            token_info: TokenInfo {
                token_id: row.get("token_id"),
                name: row.get("token_name"),
                symbol: row.get("symbol"),
                image_uri: row.get("token_image_uri"),
            },
        });

        Ok(result)
    }

    pub async fn get_new_content(&self) -> Result<NewContentResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = "new_content:all";
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = NewContentController::new(db);
                controller.fetch_new_content().await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!("get_new_content() completed in {:?}", elapsed);
        
        Ok(response)
    }
    
    async fn fetch_new_content(&self) -> Result<NewContentResponse> {

        let (new_buy, new_sell, new_token) = tokio::try_join!(
            self.get_latest_new_buy(),
            self.get_latest_new_sell(),
            self.get_latest_new_token()
        )?;

        let response = NewContentResponse {
            new_buy,
            new_sell,
            new_token,
        };

        Ok(response)
    }
}
