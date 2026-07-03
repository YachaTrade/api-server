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

    async fn fetch_total_count(&self, account_id: &str, v1_ids: &[String]) -> Result<i64> {
        let count = measure_postgres!(
            "token_created.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT mm.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM unnest($2::varchar[]) AS u(token_id)
                ) mm
                JOIN token t  ON t.token_id = mm.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .bind(v1_ids)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token created count: {}", err))?;

        Ok(count.count)
    }

    pub async fn get_tokens_created(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        v1_lp: &[(String, BigDecimal)],
    ) -> Result<CreatedTokensResponse> {
        let cache_key = cache_key!(
            "tokens_created",
            account_id,
            pagination.page,
            pagination.limit
        );

        let v1_lp_owned: Vec<(String, BigDecimal)> = v1_lp.to_vec();

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
                    .fetch_tokens_created(&account_id, &pagination, &v1_lp_owned)
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
        v1_lp: &[(String, BigDecimal)],
    ) -> Result<CreatedTokensResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let v1_ids: Vec<String> = v1_lp.iter().map(|(id, _)| id.clone()).collect();
        let v1_amts: Vec<BigDecimal> = v1_lp.iter().map(|(_, a)| a.clone()).collect();

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
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
        }

        let tokens = measure_postgres!(
            "token_created.fetch_tokens_created",
            sqlx::query_as::<_, TokenCreatedRow>(
                r#"
                WITH v1_lp AS (
                    SELECT token_id, amt FROM unnest($4::varchar[], $5::numeric[]) AS u(token_id, amt)
                ),
                member_ids AS (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM v1_lp
                ),
                paged_tokens AS (
                    SELECT t.* FROM token t
                    WHERE t.token_id IN (SELECT token_id FROM member_ids)
                      AND EXISTS (SELECT 1 FROM market mk WHERE mk.token_id = t.token_id)
                    ORDER BY t.created_at DESC, t.token_id ASC
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
                    v2cfv.current_balance as v2_current_balance,
                    v2cfv.total_claimed as v2_total_claimed,
                    (
                      COALESCE(FLOOR(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                         WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                         ELSE 0 END), 0) + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(FLOOR(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                            WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                            ELSE 0 END), 0) + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM paged_tokens t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                LEFT JOIN v2_creator_fee_vault_stats v2cfv ON t.token_id = v2cfv.token_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                LEFT JOIN v1_lp v1 ON v1.token_id = t.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY t.created_at DESC, t.token_id ASC
"#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .bind(&v1_ids)
            .bind(&v1_amts)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch tokens created: {}", err))?;

        let tokens: Vec<TokenCreatedInfo> = tokens
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
                        x_verification: None,
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
                        lp_balance: row.lp_balance.normalized().to_plain_string(),
                        total_balance: row.total_balance.normalized().to_plain_string(),
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
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
                            claimed_amount: row
                                .reward_claimed_amount
                                .normalized()
                                .to_plain_string(),
                            proof: row.reward_proof,
                            claimable: row.reward_status.as_deref() == Some("AWAITING"),
                        },
                    },
                }
            })
            .collect();

        let total_count = self.fetch_total_count(account_id, &v1_ids).await?;

        Ok(CreatedTokensResponse {
            tokens,
            total_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::pagination::PaginationParams;
    use sqlx::PgPool;
    use std::sync::Arc;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const OTHER: &str = "0x000000000000000000000000000000000000Aa09";
    const CREATED_TOKEN: &str = "0x000000000000000000000000000000000000Bb01";
    const LP_TOKEN: &str = "0x000000000000000000000000000000000000Bb09";
    const POOL_ID: &str = "0x000000000000000000000000000000000000Cc09";
    const QUOTE_ID: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A";

    fn ctrl(pool: PgPool) -> TokenCreatedController {
        TokenCreatedController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn created_union_includes_lp_only_token(pool: PgPool) {
        for (a, n) in [(ACCOUNT, "me"), (OTHER, "other")] {
            sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,$2,'','') ON CONFLICT DO NOTHING")
                .bind(a).bind(n).execute(&pool).await.unwrap();
        }
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'Mine','MINE','',$2,NULL,false,false,false,100,'0xh',1000000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(CREATED_TOKEN).bind(ACCOUNT).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'Lp','LP','',$2,NULL,false,false,false,200,'0xh',1000000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(LP_TOKEN).bind(OTHER).execute(&pool).await.unwrap();
        for t in [CREATED_TOKEN, LP_TOKEN] {
            sqlx::query("INSERT INTO swap_count (token_id,count,buy_count,sell_count) VALUES ($1,0,0,0) ON CONFLICT DO NOTHING")
                .bind(t).execute(&pool).await.unwrap();
        }
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('V2_DEX',$1,NULL,0,0,1,$2,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#)
            .bind(CREATED_TOKEN).bind(QUOTE_ID).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO pool (pool_id,token0,token1,reserve0,reserve1,price,volume,value,latest_trade_at,created_at,block_number,tx_hash,total_supply) VALUES ($1,$2,$3,10000,20000,1,0,0,0,0,1,'0xtx',1000) ON CONFLICT DO NOTHING"#)
            .bind(POOL_ID).bind(LP_TOKEN).bind("0x000000000000000000000000000000000000Bb0a").execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('V2_DEX',$1,$2,10000,20000,1,$3,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#)
            .bind(LP_TOKEN).bind(POOL_ID).bind(QUOTE_ID).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,100,0,0,0,0,0,0,0,0,0,0,0,0,0,0) ON CONFLICT DO NOTHING"#)
            .bind(ACCOUNT).bind(POOL_ID).execute(&pool).await.unwrap();

        let p = PaginationParams {
            page: 1,
            limit: 10,
            direction: "DESC".to_string(),
        };
        let resp = ctrl(pool)
            .get_tokens_created(ACCOUNT, &p, &[])
            .await
            .unwrap();
        let ids: Vec<&str> = resp
            .tokens
            .iter()
            .map(|t| t.token_info.token_id.as_str())
            .collect();
        assert!(ids.contains(&CREATED_TOKEN), "created token present");
        assert!(
            ids.contains(&LP_TOKEN),
            "LP-only token present in created union"
        );
        assert_eq!(ids[0], LP_TOKEN); // created_at DESC: 200 before 100
        let lp_row = resp
            .tokens
            .iter()
            .find(|t| t.token_info.token_id == LP_TOKEN)
            .unwrap();
        assert_eq!(lp_row.balance_info.lp_balance, "1000"); // 100*10000/1000
        assert_eq!(lp_row.balance_info.total_balance, "1000");
        assert_eq!(resp.total_count, 2);
    }
}
