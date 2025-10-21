use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, TokenInfo},
        new_event::{EventType, NewEvent, NewEventResponse},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(sqlx::FromRow)]
struct SwapEventRow {
    native_amount: BigDecimal,
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_listing: bool,
    token_created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    account_id: String,
    account_nickname: String,
    account_bio: String,
    account_image_uri: String,
    account_follower_count: i32,
    account_following_count: i32,
}

#[derive(sqlx::FromRow)]
struct CreateEventRow {
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_listing: bool,
    token_created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    account_id: String,
    account_nickname: String,
    account_bio: String,
    account_image_uri: String,
    account_follower_count: i32,
    account_following_count: i32,
}

pub struct NewEventController {
    pub db: Arc<PostgresDatabase>,
}

impl NewEventController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        NewEventController { db }
    }

    pub async fn get_new_events(&self) -> Result<NewEventResponse> {
        let cache_key = "new_event:all";

        let response = with_cache(&GLOBAL_CACHE.cache, cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = NewEventController::new(db);
                controller.fetch_new_events().await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_new_events(&self) -> Result<NewEventResponse> {
        let (buy_event, sell_event, create_event) = tokio::try_join!(
            self.fetch_latest_buy_event(),
            self.fetch_latest_sell_event(),
            self.fetch_latest_create_event()
        )?;

        let mut new_events = Vec::new();
        if let Some(event) = buy_event {
            new_events.push(event);
        }
        if let Some(event) = sell_event {
            new_events.push(event);
        }
        if let Some(event) = create_event {
            new_events.push(event);
        }

        Ok(NewEventResponse { new_events })
    }

    async fn fetch_latest_buy_event(&self) -> Result<Option<NewEvent>> {
        let query = r#"
            SELECT
                s.native_amount,
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_listing,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                a.follower_count as creator_follower_count,
                a.following_count as creator_following_count,
                s.account_id,
                COALESCE(
                    CASE WHEN av2.x_handle IS NOT NULL THEN REPLACE(ax2.x_handle, '@', '#') ELSE ax2.x_handle END,
                    a2.nickname
                ) as account_nickname,
                a2.bio as account_bio,
                COALESCE(ax2.x_image_uri, a2.image_uri) as account_image_uri,
                a2.follower_count as account_follower_count,
                a2.following_count as account_following_count
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            JOIN account a2 ON s.account_id = a2.account_id
            LEFT JOIN account_x ax2 ON a2.account_id = ax2.account_id
            LEFT JOIN account_verified av2 ON ax2.x_handle = av2.x_handle
            WHERE s.is_buy = true
            ORDER BY s.created_at DESC
            LIMIT 1
        "#;

        let row_opt = measure_postgres!(
            "new_event.fetch_latest_buy",
            sqlx::query_as::<_, SwapEventRow>(query).fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get latest buy event: {}", err))?;

        Ok(row_opt.map(|row| NewEvent {
            event_type: EventType::Buy,
            amount: row.native_amount.to_plain_string(),
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_listing: row.is_listing,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
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
            account_info: AccountInfo {
                account_id: row.account_id,
                nickname: row.account_nickname,
                bio: row.account_bio,
                image_uri: row.account_image_uri,
                follower_count: row.account_follower_count,
                following_count: row.account_following_count,
            },
        }))
    }

    async fn fetch_latest_sell_event(&self) -> Result<Option<NewEvent>> {
        let query = r#"
            SELECT
                s.native_amount,
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_listing,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                a.follower_count as creator_follower_count,
                a.following_count as creator_following_count,
                s.account_id,
                COALESCE(
                    CASE WHEN av2.x_handle IS NOT NULL THEN REPLACE(ax2.x_handle, '@', '#') ELSE ax2.x_handle END,
                    a2.nickname
                ) as account_nickname,
                a2.bio as account_bio,
                COALESCE(ax2.x_image_uri, a2.image_uri) as account_image_uri,
                a2.follower_count as account_follower_count,
                a2.following_count as account_following_count
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            JOIN account a2 ON s.account_id = a2.account_id
            LEFT JOIN account_x ax2 ON a2.account_id = ax2.account_id
            LEFT JOIN account_verified av2 ON ax2.x_handle = av2.x_handle
            WHERE s.is_buy = false
            ORDER BY s.created_at DESC
            LIMIT 1
        "#;

        let row_opt = measure_postgres!(
            "new_event.fetch_latest_sell",
            sqlx::query_as::<_, SwapEventRow>(query).fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get latest sell event: {}", err))?;

        Ok(row_opt.map(|row| NewEvent {
            event_type: EventType::Sell,
            amount: row.native_amount.to_plain_string(),
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_listing: row.is_listing,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
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
            account_info: AccountInfo {
                account_id: row.account_id,
                nickname: row.account_nickname,
                bio: row.account_bio,
                image_uri: row.account_image_uri,
                follower_count: row.account_follower_count,
                following_count: row.account_following_count,
            },
        }))
    }

    async fn fetch_latest_create_event(&self) -> Result<Option<NewEvent>> {
        let query = r#"
            SELECT
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_listing,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                a.follower_count as creator_follower_count,
                a.following_count as creator_following_count,
                t.creator as account_id,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as account_nickname,
                a.bio as account_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as account_image_uri,
                a.follower_count as account_follower_count,
                a.following_count as account_following_count
            FROM token t
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            ORDER BY t.created_at DESC
            LIMIT 1
        "#;

        let row_opt = measure_postgres!(
            "new_event.fetch_latest_create",
            sqlx::query_as::<_, CreateEventRow>(query).fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get latest create event: {}", err))?;

        Ok(row_opt.map(|row| NewEvent {
            event_type: EventType::Create,
            amount: "0".to_string(),
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_listing: row.is_listing,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
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
            account_info: AccountInfo {
                account_id: row.account_id,
                nickname: row.account_nickname,
                bio: row.account_bio,
                image_uri: row.account_image_uri,
                follower_count: row.account_follower_count,
                following_count: row.account_following_count,
            },
        }))
    }
}
