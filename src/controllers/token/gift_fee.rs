use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{
                AccountInfo, BalanceInfo, FeeInfo, MarketInfo, MarketType, QuoteInfo, RewardInfo,
                TokenCreatedInfo, TokenInfo, TokenVersion,
            },
            pagination::PaginationParams,
        },
        profile::GiftFeeTokensResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

/// Tokens where the given account is bound as the gift vault receiver.
/// Mirrors `TokenCreatedController` shape — same `TokenCreatedInfo` rows,
/// just anchored on `v2_gift_vault_stats.receiver` instead of `token.creator`.
pub struct GiftFeeController {
    db: Arc<PostgresDatabase>,
}

impl GiftFeeController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let cache_key = cache_key!("gift_fee_count", account_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = GiftFeeController::new(db);
                controller.fetch_total_count(&account_id).await
            }
        })
        .await?;

        Ok(count)
    }

    async fn fetch_total_count(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "gift_fee.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM v2_gift_vault_stats g
                WHERE g.receiver = $1
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch gift fee count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_gift_fee_tokens(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<GiftFeeTokensResponse> {
        let cache_key = cache_key!(
            "gift_fee_tokens",
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
                let controller = GiftFeeController::new(db);
                controller.fetch_gift_fee_tokens(&account_id, &pagination).await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_gift_fee_tokens(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<GiftFeeTokensResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct Row {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image_uri: String,
            token_description: Option<String>,
            token_twitter: Option<String>,
            token_telegram: Option<String>,
            token_website: Option<String>,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            version: TokenVersion,
            token_created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            market_type: String,
            market_id: String,
            quote_id: String,
            token_price: BigDecimal,
            native_price: BigDecimal,
            price: BigDecimal,
            price_usd: BigDecimal,
            total_supply: BigDecimal,
            reserve_quote: BigDecimal,
            reserve_token: BigDecimal,
            volume: BigDecimal,
            ath_price: BigDecimal,
            ath_price_quote: BigDecimal,
            quote_name: String,
            quote_symbol: String,
            quote_decimals: i32,
            quote_image_uri: String,
            creator_fee_rate: Option<i16>,
            curve_protocol_fee_rate: Option<i16>,
            dex_protocol_fee_rate: Option<i16>,
            balance: BigDecimal,
            balance_created_at: i64,
            reward_amount: BigDecimal,
            reward_claimed_amount: BigDecimal,
            reward_proof: Vec<String>,
            reward_status: Option<String>,
            v2_current_balance: Option<BigDecimal>,
            v2_total_claimed: Option<BigDecimal>,
        }

        let rows = measure_postgres!(
            "gift_fee.fetch_gift_fee_tokens",
            sqlx::query_as::<_, Row>(
                r#"
                WITH paged_tokens AS (
                    SELECT g.token_id,
                           g.current_balance AS gift_current_balance,
                           g.total_claimed   AS gift_total_claimed,
                           g.updated_at      AS gift_updated_at
                    FROM v2_gift_vault_stats g
                    WHERE g.receiver = $1
                    ORDER BY g.current_balance DESC, g.updated_at DESC
                    LIMIT $2 OFFSET $3
                ),
                claimed_totals AS (
                    SELECT token_id, SUM(amount) as claimed_amount
                    FROM creator_treasury_claim_history
                    WHERE account_id = $1
                    GROUP BY token_id
                )
                SELECT
                    t.token_id,
                    t.name as token_name,
                    t.symbol as token_symbol,
                    t.image_uri as token_image_uri,
                    t.description as token_description,
                    t.twitter as token_twitter,
                    t.telegram as token_telegram,
                    t.website as token_website,
                    t.is_graduated,
                    t.is_nsfw,
                    t.is_cto,
                    t.version,
                    t.created_at as token_created_at,
                    t.creator,
                    t.token_holder_count as holder_count,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    m.market_type,
                    COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    m.price,
                    (m.price * COALESCE(lp.price, 0)) as price_usd,
                    t.total_supply,
                    COALESCE(m.reserve_quote, 0) as reserve_quote,
                    COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume,
                    m.ath_price,
                    m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name,
                    COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals,
                    COALESCE(qt.image_uri, '') as quote_image_uri,
                    fc.creator_fee_rate,
                    fc.curve_protocol_fee_rate,
                    fc.dex_protocol_fee_rate,
                    COALESCE(b.balance, 0) as balance,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    COALESCE(cr.amount, 0) as reward_amount,
                    COALESCE(ctch.claimed_amount, 0) as reward_claimed_amount,
                    COALESCE(cr.proof, ARRAY[]::TEXT[]) as reward_proof,
                    cr.status as reward_status,
                    pg.gift_current_balance as v2_current_balance,
                    pg.gift_total_claimed   as v2_total_claimed
                FROM paged_tokens pg
                JOIN token t ON t.token_id = pg.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY pg.gift_current_balance DESC, pg.gift_updated_at DESC
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch gift fee tokens: {}", err))?;

        let tokens: Vec<TokenCreatedInfo> = rows
            .into_iter()
            .map(|row| {
                let mut market_id = row.market_id.clone();
                if market_id.is_empty() {
                    if row.market_type == "CURVE" {
                        market_id = V1_BONDING_CURVE.clone();
                    } else if row.market_type == "V2_CURVE" {
                        market_id = V2_BONDING_CURVE.clone();
                    }
                }

                TokenCreatedInfo {
                    token_info: TokenInfo {
                        token_id: row.token_id.clone(),
                        name: row.token_name,
                        symbol: row.token_symbol,
                        image_uri: row.token_image_uri,
                        description: row.token_description,
                        is_graduated: row.is_graduated,
                        is_nsfw: row.is_nsfw,
                        twitter: row.token_twitter,
                        telegram: row.token_telegram,
                        website: row.token_website,
                        created_at: row.token_created_at,
                        creator: AccountInfo {
                            account_id: row.creator,
                            nickname: row.creator_nickname,
                            bio: row.creator_bio,
                            image_uri: row.creator_image_uri,
                        },
                        is_cto: row.is_cto,
                        version: row.version.clone(),
                    },
                    market_info: MarketInfo {
                        market_type: match row.market_type.as_str() {
                            "CURVE" => MarketType::Curve,
                            "DEX" => MarketType::Dex,
                            "V2_CURVE" => MarketType::V2Curve,
                            "V2_DEX" => MarketType::V2Dex,
                            _ => MarketType::Curve,
                        },
                        token_id: row.token_id,
                        quote_info: QuoteInfo {
                            quote_id: row.quote_id.clone(),
                            name: row.quote_name.clone(),
                            symbol: row.quote_symbol.clone(),
                            decimals: row.quote_decimals as u32,
                            image_uri: row.quote_image_uri.clone(),
                        },
                        market_id,
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
                        price: row.price.normalized().to_plain_string(),
                        price_usd: row.price_usd.normalized().to_plain_string(),
                        price_native: row.price.normalized().to_plain_string(),
                        price_quote: row.price.normalized().to_plain_string(),
                        total_supply: row.total_supply.normalized().to_plain_string(),
                        reserve_native: row.reserve_quote.normalized().to_plain_string(),
                        reserve_quote: row.reserve_quote.normalized().to_plain_string(),
                        reserve_token: row.reserve_token.normalized().to_plain_string(),
                        volume: row.volume.normalized().to_plain_string(),
                        ath_price: row.ath_price.normalized().to_plain_string(),
                        ath_price_usd: row.ath_price.normalized().to_plain_string(),
                        ath_price_native: row.ath_price_quote.normalized().to_plain_string(),
                        ath_price_quote: row.ath_price_quote.normalized().to_plain_string(),
                        holder_count: row.holder_count,
                        fee_info: match row.market_type.as_str() {
                            "V2_CURVE" | "V2_DEX" => Some(FeeInfo {
                                creator_protocol_fee_rate: row.creator_fee_rate.unwrap_or(0),
                                curve_protocol_fee_rate: row.curve_protocol_fee_rate.unwrap_or(0),
                                dex_protocol_fee_rate: row.dex_protocol_fee_rate.unwrap_or(0),
                            }),
                            _ => None,
                        },
                    },
                    balance_info: BalanceInfo {
                        balance: row.balance.normalized().to_plain_string(),
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        created_at: row.balance_created_at,
                    },
                    reward_info: match row.version {
                        TokenVersion::V2 => {
                            let current_balance = row
                                .v2_current_balance
                                .clone()
                                .unwrap_or_else(|| BigDecimal::from(0));
                            let claimable = current_balance > BigDecimal::from(0);
                            RewardInfo {
                                amount: current_balance.normalized().to_plain_string(),
                                claimed_amount: row
                                    .v2_total_claimed
                                    .clone()
                                    .unwrap_or_else(|| BigDecimal::from(0))
                                    .normalized()
                                    .to_plain_string(),
                                proof: vec![],
                                claimable,
                            }
                        }
                        TokenVersion::V1 => RewardInfo {
                            amount: row.reward_amount.normalized().to_plain_string(),
                            claimed_amount: row.reward_claimed_amount.normalized().to_plain_string(),
                            proof: row.reward_proof,
                            claimable: row.reward_status.as_deref() == Some("AWAITING"),
                        },
                    },
                }
            })
            .collect();

        let total_count = self.fetch_total_count(account_id).await?;

        Ok(GiftFeeTokensResponse {
            tokens,
            total_count,
        })
    }
}
