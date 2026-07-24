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

const MIN_NEW_EVENT_QUOTE_AMOUNT: &str = "0.0001";

#[derive(sqlx::FromRow)]
struct SwapEventRow {
    swap_created_at: i64,
    quote_amount: BigDecimal,
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
        let minimum_quote_amount = MIN_NEW_EVENT_QUOTE_AMOUNT
            .parse::<BigDecimal>()
            .expect("MIN_NEW_EVENT_QUOTE_AMOUNT must be a valid decimal");

        let query = r#"
            SELECT
                s.created_at as swap_created_at,
                s.quote_amount,
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
                t.created_at as token_created_at,
                t.creator,
                a.nickname as creator_nickname,
                a.bio as creator_bio,
                a.image_uri as creator_image_uri,
                s.account_id,
                a2.nickname as account_nickname,
                a2.bio as account_bio,
                a2.image_uri as account_image_uri
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            JOIN account a2 ON s.account_id = a2.account_id
            JOIN market m ON s.token_id = m.token_id
            JOIN quote_token qt ON m.quote_id = qt.quote_id
            WHERE s.is_buy = true
              AND s.quote_amount >= CEIL(
                  $2 * POWER(10::NUMERIC, qt.decimals)
              )
            ORDER BY s.created_at DESC
            LIMIT $1
        "#;

        let rows = measure_postgres!(
            "new_event.fetch_buy_events",
            sqlx::query_as::<_, SwapEventRow>(query)
                .bind(limit)
                .bind(&minimum_quote_amount)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get buy events: {}", err))?;

        Ok(rows
            .into_iter()
            .map(|row| NewEvent {
                event_type: EventType::Buy,
                amount: row.quote_amount.normalized().to_plain_string(),
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
        let minimum_quote_amount = MIN_NEW_EVENT_QUOTE_AMOUNT
            .parse::<BigDecimal>()
            .expect("MIN_NEW_EVENT_QUOTE_AMOUNT must be a valid decimal");

        let query = r#"
            SELECT
                s.created_at as swap_created_at,
                s.quote_amount,
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
                t.created_at as token_created_at,
                t.creator,
                a.nickname as creator_nickname,
                a.bio as creator_bio,
                a.image_uri as creator_image_uri,
                s.account_id,
                a2.nickname as account_nickname,
                a2.bio as account_bio,
                a2.image_uri as account_image_uri
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            JOIN account a2 ON s.account_id = a2.account_id
            JOIN market m ON s.token_id = m.token_id
            JOIN quote_token qt ON m.quote_id = qt.quote_id
            WHERE s.is_buy = false
              AND s.quote_amount >= CEIL(
                  $2 * POWER(10::NUMERIC, qt.decimals)
              )
            ORDER BY s.created_at DESC
            LIMIT $1
        "#;

        let rows = measure_postgres!(
            "new_event.fetch_sell_events",
            sqlx::query_as::<_, SwapEventRow>(query)
                .bind(limit)
                .bind(&minimum_quote_amount)
                .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get sell events: {}", err))?;

        Ok(rows
            .into_iter()
            .map(|row| NewEvent {
                event_type: EventType::Sell,
                amount: row.quote_amount.normalized().to_plain_string(),
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
                t.created_at as token_created_at,
                t.creator,
                a.nickname as creator_nickname,
                a.bio as creator_bio,
                a.image_uri as creator_image_uri,
                t.creator as account_id,
                a.nickname as account_nickname,
                a.bio as account_bio,
                a.image_uri as account_image_uri
            FROM token t
            JOIN account a ON t.creator = a.account_id
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn addr(suffix: &str) -> String {
        format!("0x{:0>40}", suffix)
    }

    fn controller(pool: PgPool) -> NewEventController {
        NewEventController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_account(pool: &PgPool, account_id: &str) {
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri)
             VALUES ($1, 'account', '', '')
             ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_quote(pool: &PgPool, quote_id: &str, decimals: i32) {
        sqlx::query(
            "INSERT INTO quote_token
                (quote_id, name, symbol, decimals, pyth_feed_id, image_uri)
             VALUES ($1, 'Quote', 'Q', $2, 'feed', '')
             ON CONFLICT (quote_id)
             DO UPDATE SET decimals = EXCLUDED.decimals",
        )
        .bind(quote_id)
        .bind(decimals)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_token_market(pool: &PgPool, token_id: &str, creator: &str, quote_id: &str) {
        sqlx::query(
            "INSERT INTO token
                (token_id, name, symbol, image_uri, creator, created_at,
                 transaction_hash, total_supply)
             VALUES ($1, 'Token', 'T', '', $2, 0, $3, 0)",
        )
        .bind(token_id)
        .bind(creator)
        .bind(format!("token-{token_id}"))
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO market
                (market_type, token_id, price, quote_id, latest_trade_at, created_at)
             VALUES ('DEX', $1, 1, $2, 0, 0)",
        )
        .bind(token_id)
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_swap(
        pool: &PgPool,
        account_id: &str,
        token_id: &str,
        is_buy: bool,
        quote_amount: &str,
        created_at: i64,
        transaction_hash: &str,
    ) {
        sqlx::query(
            "INSERT INTO swap
                (account_id, token_id, market_type, is_buy, quote_amount,
                 token_amount, created_at, transaction_hash, tx_index, log_index)
             VALUES ($1, $2, 'DEX', $3, $4::NUMERIC, 0, $5, $6, 0, 0)",
        )
        .bind(account_id)
        .bind(token_id)
        .bind(is_buy)
        .bind(quote_amount)
        .bind(created_at)
        .bind(transaction_hash)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn new_event_threshold_uses_each_quote_tokens_decimals(pool: PgPool) {
        let account = addr("acc1");
        let quote_18 = addr("e018");
        let quote_6 = addr("e006");
        let token_18 = addr("a018");
        let token_6 = addr("a006");

        seed_account(&pool, &account).await;
        seed_quote(&pool, &quote_18, 18).await;
        seed_quote(&pool, &quote_6, 6).await;
        seed_token_market(&pool, &token_18, &account, &quote_18).await;
        seed_token_market(&pool, &token_6, &account, &quote_6).await;

        seed_swap(
            &pool,
            &account,
            &token_18,
            true,
            "99999999999999",
            20,
            "buy-below",
        )
        .await;
        seed_swap(
            &pool,
            &account,
            &token_18,
            true,
            "100000000000000",
            10,
            "buy-at",
        )
        .await;
        seed_swap(&pool, &account, &token_6, false, "99", 20, "sell-below").await;
        seed_swap(&pool, &account, &token_6, false, "100", 10, "sell-at").await;

        let controller = controller(pool);
        let buys = controller.fetch_buy_events(4).await.unwrap();
        let sells = controller.fetch_sell_events(4).await.unwrap();

        let buy_amounts: Vec<&str> = buys.iter().map(|event| event.amount.as_str()).collect();
        let sell_amounts: Vec<&str> = sells.iter().map(|event| event.amount.as_str()).collect();

        assert_eq!(buy_amounts, vec!["100000000000000"]);
        assert_eq!(sell_amounts, vec!["100"]);
    }
}
