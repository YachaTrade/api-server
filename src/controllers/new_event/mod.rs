use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, TokenInfo, TokenVersion},
        new_event::{EventType, NewEvent, NewEventResponse},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(sqlx::FromRow)]
struct SwapEventRow {
    swap_created_at: i64,
    native_amount: BigDecimal,
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    token_created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    account_id: String,
    account_nickname: String,
    account_bio: String,
    account_image_uri: String,
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
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    token_created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    account_id: String,
    account_nickname: String,
    account_bio: String,
    account_image_uri: String,
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
        let (create_events, buy_events, sell_events) = tokio::try_join!(
            self.fetch_create_events(2),
            self.fetch_buy_events(4),
            self.fetch_sell_events(4)
        )?;

        let mut all_events: Vec<NewEvent> = create_events
            .into_iter()
            .chain(buy_events.into_iter())
            .chain(sell_events.into_iter())
            .collect();

        // Sort by event_created_at (swap.created_at for buy/sell, token.created_at for create)
        all_events.sort_by(|a, b| b.event_created_at.cmp(&a.event_created_at));

        Ok(NewEventResponse {
            new_events: all_events,
        })
    }

    async fn fetch_buy_events(&self, limit: i64) -> Result<Vec<NewEvent>> {
        let query = r#"
            SELECT
                s.created_at as swap_created_at,
                s.native_amount,
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_graduated,
                t.is_nsfw,
                t.is_cto,
                        t.version,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                s.account_id,
                COALESCE(ax2.x_handle, a2.nickname) as account_nickname,
                a2.bio as account_bio,
                COALESCE(ax2.x_image_uri, a2.image_uri) as account_image_uri
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            JOIN account a2 ON s.account_id = a2.account_id
            LEFT JOIN account_x ax2 ON a2.account_id = ax2.account_id
            WHERE s.is_buy = true AND s.native_amount >= 1000000000000000000
            ORDER BY s.created_at DESC
            LIMIT $1
        "#;

        let rows = measure_postgres!(
            "new_event.fetch_buy_events",
            sqlx::query_as::<_, SwapEventRow>(query)
                .bind(limit)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get buy events: {}", err))?;

        Ok(rows
            .into_iter()
            .map(|row| NewEvent {
                event_type: EventType::Buy,
                amount: row.native_amount.normalized().to_plain_string(),
                token_info: TokenInfo {
                    token_id: row.token_id.clone(),
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.token_image_uri,
                    description: row.description,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: row.twitter,
                    telegram: row.telegram,
                    website: row.website,
                    created_at: row.token_created_at,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                    },
                    is_cto: row.is_cto,
                    version: row.version.clone(),
                    hackathon_info: None,
                },
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.account_nickname,
                    bio: row.account_bio,
                    image_uri: row.account_image_uri,
                },
                event_created_at: row.swap_created_at,
            })
            .collect())
    }

    async fn fetch_sell_events(&self, limit: i64) -> Result<Vec<NewEvent>> {
        let query = r#"
            SELECT
                s.created_at as swap_created_at,
                s.native_amount,
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                t.twitter,
                t.telegram,
                t.website,
                t.is_graduated,
                t.is_nsfw,
                t.is_cto,
                        t.version,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                s.account_id,
                COALESCE(ax2.x_handle, a2.nickname) as account_nickname,
                a2.bio as account_bio,
                COALESCE(ax2.x_image_uri, a2.image_uri) as account_image_uri
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            JOIN account a2 ON s.account_id = a2.account_id
            LEFT JOIN account_x ax2 ON a2.account_id = ax2.account_id
            WHERE s.is_buy = false AND s.native_amount >= 1000000000000000000
            ORDER BY s.created_at DESC
            LIMIT $1
        "#;

        let rows = measure_postgres!(
            "new_event.fetch_sell_events",
            sqlx::query_as::<_, SwapEventRow>(query)
                .bind(limit)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get sell events: {}", err))?;

        Ok(rows
            .into_iter()
            .map(|row| NewEvent {
                event_type: EventType::Sell,
                amount: row.native_amount.normalized().to_plain_string(),
                token_info: TokenInfo {
                    token_id: row.token_id.clone(),
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.token_image_uri,
                    description: row.description,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: row.twitter,
                    telegram: row.telegram,
                    website: row.website,
                    created_at: row.token_created_at,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                    },
                    is_cto: row.is_cto,
                    version: row.version.clone(),
                    hackathon_info: None,
                },
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.account_nickname,
                    bio: row.account_bio,
                    image_uri: row.account_image_uri,
                },
                event_created_at: row.swap_created_at,
            })
            .collect())
    }

    async fn fetch_create_events(&self, limit: i64) -> Result<Vec<NewEvent>> {
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
                t.is_graduated,
                t.is_nsfw,
                t.is_cto,
                        t.version,
                t.created_at as token_created_at,
                t.creator,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                t.creator as account_id,
                COALESCE(ax.x_handle, a.nickname) as account_nickname,
                a.bio as account_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as account_image_uri
            FROM token t
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            ORDER BY t.created_at DESC
            LIMIT $1
        "#;

        let rows = measure_postgres!(
            "new_event.fetch_create_events",
            sqlx::query_as::<_, CreateEventRow>(query)
                .bind(limit)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get create events: {}", err))?;

        Ok(rows
            .into_iter()
            .map(|row| NewEvent {
                event_type: EventType::Create,
                amount: "0".to_string(),
                token_info: TokenInfo {
                    token_id: row.token_id.clone(),
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.token_image_uri,
                    description: row.description,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: row.twitter,
                    telegram: row.telegram,
                    website: row.website,
                    created_at: row.token_created_at,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                    },
                    is_cto: row.is_cto,
                    version: row.version.clone(),
                    hackathon_info: None,
                },
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.account_nickname,
                    bio: row.account_bio,
                    image_uri: row.account_image_uri,
                },
                event_created_at: row.token_created_at,
            })
            .collect())
    }
}
