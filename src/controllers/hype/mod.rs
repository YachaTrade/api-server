use std::{str::FromStr, sync::Arc};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{AccountInfo, TokenInfo, TokenVersion},
            pagination::PaginationParams,
        },
        hype::{
            AmountResponse, HypeEpochResponse, HypeInfo, HypePointResponse,
            HypeRewardAddHistoryResponse, HypeToken, HypeTokenResponse, HypeVoteHistory,
            HypeVoteHistoryResponse, HypeVoteRequest, HypeVoteResponse, RewardAdd,
        },
        profile::{PointHistoryResponse, PointRecord},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, sqlx::FromRow)]
struct AmountRow {
    amount: BigDecimal,
}

#[derive(Debug, sqlx::FromRow)]
struct HypeTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    image_uri: String,
    description: Option<String>,
    is_graduated: bool,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    #[allow(dead_code)]
    total_supply: BigDecimal,
    created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    vote: BigDecimal,
    holder_count: i64,
    market_cap: BigDecimal,
    market_cap_usd: BigDecimal,
    reward_amount: Option<BigDecimal>,
}

pub struct HypeController {
    pub db: Arc<PostgresDatabase>,
}

impl HypeController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        HypeController { db }
    }

    pub async fn get_active_epoch(&self) -> Result<Option<HypeEpochResponse>> {
        #[derive(sqlx::FromRow)]
        struct ActiveEpochRow {
            epoch: i64,
            start_at: i64,
            end_at: i64,
            status: String,
        }

        let row = measure_postgres!(
            "hype.get_active_epoch",
            sqlx::query_as::<_, ActiveEpochRow>(
                r#"
                SELECT epoch, start_at, end_at, status
                FROM epoch
                WHERE status = 'ACTIVE'
                LIMIT 1
                "#,
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch active hype epoch: {}", err))?;

        Ok(row.map(|row| HypeEpochResponse {
            epoch: row.epoch,
            start_at: row.start_at,
            end_at: row.end_at,
            status: row.status,
        }))
    }

    pub async fn get_hype_token(&self) -> Result<HypeTokenResponse> {
        let cache_key = "hype_token";

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_token().await
            }
        })
        .await?;

        Ok(response)
    }

    pub async fn get_hype_token_latest(&self) -> Result<HypeTokenResponse> {
        let cache_key = "hype_token_latest";

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_token_latest().await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_token_latest(&self) -> Result<HypeTokenResponse> {
        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_token_latest.rows",
                sqlx::query_as::<_, HypeTokenRow>(
                    r#"
                    WITH latest_price AS (
                        SELECT price FROM price ORDER BY created_at DESC LIMIT 1
                    )
                    SELECT
                        h.vote,
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.description,
                        t.is_graduated,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.total_supply,
                        t.created_at,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        (m.price * t.total_supply * COALESCE(lp.price, 0)) as market_cap_usd,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                    CROSS JOIN latest_price lp
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETED' ORDER BY epoch DESC LIMIT 1)
                    )
                    ORDER BY h.vote DESC, market_cap DESC
                    "#,
                )
                .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_token_latest.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COUNT(*) as count
                    FROM hype_token h
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETED' ORDER BY epoch DESC LIMIT 1)
                    )
                    "#,
                )
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let token_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch latest hype tokens: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch latest hype token count: {}", err))?
            .count as u64;

        let tokens = token_rows
            .into_par_iter()
            .map(|row| HypeToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
                    market_cap_usd: row.market_cap_usd.normalized().to_plain_string(),
                    reward_amount: row
                        .reward_amount
                        .unwrap_or_default()
                        .normalized()
                        .to_plain_string(),
                },
            })
            .collect::<Vec<HypeToken>>();

        Ok(HypeTokenResponse {
            tokens,
            total_count,
        })
    }

    async fn fetch_hype_token(&self) -> Result<HypeTokenResponse> {
        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_token.rows",
                sqlx::query_as::<_, HypeTokenRow>(
                    r#"
                    WITH latest_price AS (
                        SELECT price FROM price ORDER BY created_at DESC LIMIT 1
                    )
                    SELECT
                        h.vote,
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.description,
                        t.is_graduated,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.total_supply,
                        t.created_at,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        (m.price * t.total_supply * COALESCE(lp.price, 0)) as market_cap_usd,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                    CROSS JOIN latest_price lp
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETED' ORDER BY epoch DESC LIMIT 1)
                    )
                    ORDER BY h.vote DESC, market_cap DESC
                    "#,
                )
                .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_token.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COUNT(*) as count
                    FROM hype_token h
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETED' ORDER BY epoch DESC LIMIT 1)
                    )
                    "#,
                )
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let token_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch hype tokens: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype token count: {}", err))?
            .count as u64;

        let tokens = token_rows
            .into_par_iter()
            .map(|row| HypeToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
                    market_cap_usd: row.market_cap_usd.normalized().to_plain_string(),
                    reward_amount: row
                        .reward_amount
                        .unwrap_or_default()
                        .normalized()
                        .to_plain_string(),
                },
            })
            .collect::<Vec<HypeToken>>();

        Ok(HypeTokenResponse {
            tokens,
            total_count,
        })
    }

    pub async fn get_hype_point(&self, account_id: &str) -> Result<HypePointResponse> {
        let cache_key = format!("hype_point:{}", account_id);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_point(&account_id).await
            }
        })
        .await?;

        Ok(response)
    }

    pub async fn get_hype_token_epoch(&self, epoch: i64) -> Result<HypeTokenResponse> {
        let cache_key = format!("hype_token_epoch:{}", epoch);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_token_epoch(epoch).await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_token_epoch(&self, epoch: i64) -> Result<HypeTokenResponse> {
        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_token_epoch.rows",
                sqlx::query_as::<_, HypeTokenRow>(
                    r#"
                    WITH latest_price AS (
                        SELECT price FROM price ORDER BY created_at DESC LIMIT 1
                    )
                    SELECT
                        h.vote,
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.description,
                        t.is_graduated,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.total_supply,
                        t.created_at,
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        (m.price * t.total_supply * COALESCE(lp.price, 0)) as market_cap_usd,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                    CROSS JOIN latest_price lp
                    WHERE h.epoch = $1
                    ORDER BY h.vote DESC, market_cap DESC
                    "#,
                )
                .bind(epoch)
                .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_token_epoch.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COUNT(*) as count
                    FROM hype_token h
                    WHERE h.epoch = $1
                    "#,
                )
                .bind(epoch)
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let token_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch hype tokens for epoch: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype token count for epoch: {}", err))?
            .count as u64;

        let tokens = token_rows
            .into_par_iter()
            .map(|row| HypeToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
                    market_cap_usd: row.market_cap_usd.normalized().to_plain_string(),
                    reward_amount: row
                        .reward_amount
                        .unwrap_or_default()
                        .normalized()
                        .to_plain_string(),
                },
            })
            .collect::<Vec<HypeToken>>();

        Ok(HypeTokenResponse {
            tokens,
            total_count,
        })
    }

    async fn fetch_hype_point(&self, account_id: &str) -> Result<HypePointResponse> {
        #[derive(sqlx::FromRow)]
        struct PointRow {
            account_id: String,
            round_point: i64,
            hype_point: i64,
        }

        let row = measure_postgres!(
            "hype.fetch_hype_point",
            sqlx::query_as::<_, PointRow>(
                r#"
                SELECT
                    account_id,
                    round_point,
                    hype_point
                FROM point
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch hype point: {}", err))?;

        match row {
            Some(row) => Ok(HypePointResponse {
                account_id: row.account_id,
                round_point: row.round_point.to_string(),
                hype_point: row.hype_point.to_string(),
            }),
            None => Ok(HypePointResponse {
                account_id: account_id.to_string(),
                round_point: "0".to_string(),
                hype_point: "0".to_string(),
            }),
        }
    }

    pub async fn get_hype_epoch(&self) -> Result<HypeEpochResponse> {
        let cache_key = "hype_epoch";

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_epoch().await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_epoch(&self) -> Result<HypeEpochResponse> {
        #[derive(sqlx::FromRow)]
        struct HypeEpochRow {
            epoch: i64,
            start_at: i64,
            end_at: i64,
            status: String,
        }
        let row = measure_postgres!(
            "hype.fetch_hype_epoch",
            sqlx::query_as::<_, HypeEpochRow>(
                r#"
                SELECT
                    epoch,
                    start_at,
                    end_at,
                    status
                FROM epoch
                WHERE epoch = COALESCE(
                    (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                    (SELECT MIN(epoch) FROM epoch WHERE status = 'READY')
                )
                "#,
            )
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch hype epoch: {}", err))?;

        Ok(HypeEpochResponse {
            epoch: row.epoch,
            start_at: row.start_at,
            end_at: row.end_at,
            status: row.status,
        })
    }

    pub async fn get_hype_vote_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeVoteHistoryResponse> {
        let cache_key = format!(
            "hype_vote_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();

            async move {
                let controller = HypeController::new(db);
                controller
                    .fetch_hype_vote_history(&account_id, pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_vote_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeVoteHistoryResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        #[derive(sqlx::FromRow)]
        struct HypeVoteHistoryRow {
            epoch: i64,
            epoch_status: String,
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            token_created_at: i64,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            version: TokenVersion,
            vote_amount: BigDecimal,
            creator: String,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            reward_amount: Option<BigDecimal>,
            reward_status: Option<String>,
            reward_proof: Option<Vec<String>>,
        }

        let query = r#"
            WITH vote_summary AS (
                SELECT
                    epoch,
                    token_id,
                    SUM(vote) as vote_amount
                FROM vote_history
                WHERE account_id = $1
                GROUP BY epoch, token_id
            )
            SELECT
                vs.epoch,
                e.status as epoch_status,
                vs.token_id,
                vs.vote_amount,
                t.name,
                t.symbol,
                t.image_uri,
                t.created_at as token_created_at,
                t.creator,
                t.is_nsfw,
                t.is_graduated,
                t.is_cto,
                        t.version,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                r.amount as reward_amount,
                r.status as reward_status,
                r.proof as reward_proof
            FROM vote_summary vs
            JOIN epoch e ON vs.epoch = e.epoch
            JOIN token t ON vs.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN reward r ON vs.epoch = r.epoch
                AND vs.token_id = r.token_id
                AND r.account_id = $1
            ORDER BY vs.epoch DESC, vs.vote_amount DESC
            LIMIT $2 OFFSET $3
            "#;

        let count_query = r#"
            SELECT COALESCE(total_count, 0) as count
            FROM account_vote_history_count
            WHERE account_id = $1
            "#;

        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_vote_history.rows",
                sqlx::query_as::<_, HypeVoteHistoryRow>(query)
                    .bind(account_id)
                    .bind(pagination.limit)
                    .bind(offset)
                    .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_vote_history.count",
                sqlx::query_as::<_, CountRow>(count_query)
                    .bind(account_id)
                    .fetch_optional(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let vote_history_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch hype vote history: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype vote history count: {}", err))?
            .map(|row| row.count as u64)
            .unwrap_or(0);

        let history = vote_history_rows
            .into_iter()
            .map(|row| HypeVoteHistory {
                epoch: row.epoch,
                is_live: row.epoch_status == "ACTIVE",
                token_info: TokenInfo {
                    token_id: row.token_id.clone(),
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    description: None,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: None,
                    telegram: None,
                    website: None,
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
                vote_amount: row.vote_amount.normalized().to_plain_string(),
                reward_amount: row
                    .reward_amount
                    .unwrap_or_default()
                    .normalized()
                    .to_plain_string(),
                claimable: row.reward_status.as_deref() == Some("AWAITING"),
                proof: row.reward_proof.unwrap_or_default(),
            })
            .collect();

        Ok(HypeVoteHistoryResponse {
            history,
            total_count,
        })
    }

    pub async fn get_hype_point_history(
        &self,
        account_id: &str,
        pagination_params: &PaginationParams,
    ) -> Result<PointHistoryResponse> {
        let cache_key = format!(
            "hype_point_history:{}:{}:{}",
            account_id, pagination_params.page, pagination_params.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination_params;
            async move {
                let controller = HypeController::new(db);
                controller
                    .fetch_hype_point_history(&account_id, pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_point_history(
        &self,
        account_id: &str,
        pagination_params: &PaginationParams,
    ) -> Result<PointHistoryResponse> {
        let offset = (pagination_params.page - 1) * pagination_params.limit;

        #[derive(sqlx::FromRow)]
        struct PointDistributionGroupRow {
            created_at: i64,
            total_point: BigDecimal,
            details: sqlx::types::Json<Vec<PointDetailJson>>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct PointDetailJson {
            epoch: i64,
            activity_type: String,
            amount: BigDecimal,
        }

        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_point_history.rows",
                sqlx::query_as::<_, PointDistributionGroupRow>(
                    r#"
                    SELECT
                        created_at,
                        SUM(amount) as total_point,
                        jsonb_agg(
                            jsonb_build_object(
                                'epoch', epoch,
                                'activity_type', activity_type,
                                'amount', amount
                            )
                            ORDER BY amount DESC
                        ) as details
                    FROM point_distribution
                    WHERE account_id = $1
                    GROUP BY created_at
                    ORDER BY created_at DESC
                    LIMIT $2 OFFSET $3
                    "#,
                )
                .bind(account_id)
                .bind(pagination_params.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_point_history.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COUNT(DISTINCT created_at) as count
                    FROM point_distribution
                    WHERE account_id = $1
                    "#,
                )
                .bind(account_id)
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let point_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch hype point history: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype point history count: {}", err))?
            .count as u64;

        let histories = point_rows
            .into_iter()
            .map(|row| {
                use crate::types::profile::PointRecordTotal;

                PointRecordTotal {
                    total_point: row.total_point.to_string(),
                    history: row
                        .details
                        .0
                        .into_iter()
                        .map(|detail| PointRecord {
                            epoch: detail.epoch,
                            activity_type: detail.activity_type,
                            amount: detail.amount.to_string(),
                            created_at: row.created_at,
                        })
                        .collect(),
                    created_at: row.created_at,
                }
            })
            .collect();

        Ok(PointHistoryResponse {
            histories,
            total_count,
        })
    }

    pub async fn vote(
        &self,
        account_id: &str,
        payload: &HypeVoteRequest,
    ) -> Result<HypeVoteResponse> {
        // Amount validation is done in service layer
        let amount = payload
            .amount
            .parse::<i64>()
            .map_err(|_| anyhow!("Invalid amount format"))?;

        #[derive(sqlx::FromRow)]
        struct VoteResult {
            new_round_point: i64,
            new_hype_point: i64,
            new_vote: BigDecimal,
        }

        // Lock both point and hype_token rows to prevent race conditions
        let result = sqlx::query_as::<_, VoteResult>(
            r#"
            WITH active_epoch AS (
                SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1
            ),
            locked_point AS (
                SELECT round_point, hype_point
                FROM point
                WHERE account_id = $1
                FOR UPDATE
            ),
            locked_hype_token AS (
                SELECT vote
                FROM hype_token
                WHERE epoch = (SELECT epoch FROM active_epoch)
                AND token_id = $2
                FOR UPDATE
            ),
            vote_history_insert AS (
                INSERT INTO vote_history (epoch, token_id, account_id, vote, total_vote_amount)
                SELECT
                    (SELECT epoch FROM active_epoch),
                    $2,
                    $1,
                    $3,
                    (SELECT vote + $3 FROM locked_hype_token)
                WHERE (SELECT round_point FROM locked_point) >= $3
                AND EXISTS (SELECT 1 FROM locked_hype_token)
                RETURNING id
            ),
            point_update AS (
                UPDATE point
                SET round_point = round_point - $3, hype_point = hype_point + $3
                WHERE account_id = $1 AND round_point >= $3
                AND EXISTS (SELECT 1 FROM vote_history_insert)
                RETURNING round_point as new_round_point, hype_point as new_hype_point
            ),
            vote_update AS (
                UPDATE hype_token
                SET vote = vote + $3
                WHERE epoch = (SELECT epoch FROM active_epoch)
                AND token_id = $2
                AND EXISTS (SELECT 1 FROM vote_history_insert)
                RETURNING vote as new_vote
            )
            SELECT
                (SELECT new_round_point FROM point_update) as new_round_point,
                (SELECT new_hype_point FROM point_update) as new_hype_point,
                (SELECT new_vote FROM vote_update) as new_vote
            WHERE EXISTS (SELECT 1 FROM vote_history_insert)
            "#,
        )
        .bind(account_id)
        .bind(&payload.token_id)
        .bind(amount)
        .fetch_optional(self.db.get_write_pool())
        .await?;

        let vote_result = result.ok_or_else(|| anyhow!("Insufficient points for voting"))?;

        Ok(HypeVoteResponse {
            account_id: account_id.to_string(),
            round_point: vote_result.new_round_point.to_string(),
            hype_point: vote_result.new_hype_point.to_string(),
            token_vote: vote_result.new_vote.to_string(),
        })
    }

    pub async fn get_total_hype_point(&self) -> Result<AmountResponse> {
        let cache_key = "get_total_hype_point";

        with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_total_hype_point().await
            }
        })
        .await
    }

    async fn fetch_total_hype_point(&self) -> Result<AmountResponse> {
        let query = sqlx::query_as::<_, AmountRow>(
            "SELECT hype_point::NUMERIC as amount FROM total_hype_point WHERE id = 1",
        )
        .fetch_one(self.db.get_read_pool());

        let result = measure_postgres!("hype.total_hype_point", query)
            .map_err(|err| anyhow!("Failed to get total hype point\n Reason: {err}"))?;

        Ok(AmountResponse {
            amount: result.amount.to_string(),
        })
    }

    pub async fn get_community_treasury(&self) -> Result<AmountResponse> {
        let cache_key = "get_community_treasury";

        with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_community_treasury().await
            }
        })
        .await
    }

    async fn fetch_community_treasury(&self) -> Result<AmountResponse> {
        let wmon_balance = self.get_wmon_balance().await.unwrap_or_else(|e| {
            tracing::error!("Failed to get WMON balance: {}", e);
            BigDecimal::from(0)
        });

        Ok(AmountResponse {
            amount: wmon_balance.normalized().to_plain_string(),
        })
    }

    async fn get_wmon_balance(&self) -> Result<bigdecimal::BigDecimal> {
        use crate::config::{COMMUNITY_TREASURY, RPC_URL, WMON};
        use alloy::primitives::{Address, U256};
        use alloy::providers::ProviderBuilder;
        use alloy::sol;

        sol! {
            #[allow(missing_docs)]
            #[sol(rpc)]
            interface IERC20 {
                function balanceOf(address account) external view returns (uint256);
            }
        }

        let rpc_url: url::Url = RPC_URL
            .parse()
            .map_err(|e| anyhow!("Invalid RPC_URL: {}", e))?;

        let provider = ProviderBuilder::new().connect_http(rpc_url);

        let wmon_address: Address = WMON
            .parse()
            .map_err(|e| anyhow!("Invalid WMON address: {}", e))?;
        let treasury_address: Address = COMMUNITY_TREASURY
            .parse()
            .map_err(|e| anyhow!("Invalid COMMUNITY_TREASURY address: {}", e))?;

        let contract = IERC20::new(wmon_address, provider);

        let balance_result = contract.balanceOf(treasury_address).call().await?;
        let balance: U256 = balance_result;

        let balance_str = balance.to_string();
        let balance_decimal = bigdecimal::BigDecimal::from_str(&balance_str)
            .map_err(|e| anyhow!("Failed to convert balance to BigDecimal: {}", e))?;

        Ok(balance_decimal)
    }

    pub async fn get_hype_reward_add_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeRewardAddHistoryResponse> {
        let cache_key = format!(
            "hype_reward_add_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();

            async move {
                let controller = HypeController::new(db);
                controller
                    .fetch_hype_reward_add_history(&account_id, pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_reward_add_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeRewardAddHistoryResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct RewardAddHistoryRow {
            epoch: i64,
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            is_graduated: bool,
            is_nsfw: bool,
            is_cto: bool,
            version: TokenVersion,
            token_created_at: i64,
            amount: BigDecimal,
            total_amount: BigDecimal,
            transaction_hash: String,
            created_at: i64,
            creator: String,
            #[allow(dead_code)]
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
        }

        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_reward_add_history.rows",
                sqlx::query_as::<_, RewardAddHistoryRow>(
                    r#"
                    SELECT
                        rah.epoch,
                        rah.token_id,
                        rah.amount,
                        rah.total_amount,
                        rah.transaction_hash,
                        rah.created_at,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at as token_created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri
                    FROM reward_add_history rah
                    JOIN token t ON rah.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    WHERE rah.account_id = $1
                    ORDER BY rah.created_at DESC
                    LIMIT $2 OFFSET $3
                    "#,
                )
                .bind(account_id)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool())
            )
        };

        let total_count_future = async {
            measure_postgres!(
                "hype.fetch_hype_reward_add_history.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COALESCE(total_count, 0) as count
                    FROM reward_add_history_count
                    WHERE account_id = $1
                    "#,
                )
                .bind(account_id)
                .fetch_optional(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let reward_add_rows = rows_result
            .map_err(|err| anyhow!("Failed to fetch hype reward add history: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype reward add history count: {}", err))?
            .map(|row| row.count as u64)
            .unwrap_or(0);

        let history = reward_add_rows
            .into_iter()
            .map(|row| RewardAdd {
                epoch: row.epoch,
                token_info: TokenInfo {
                    token_id: row.token_id.clone(),
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    description: None,
                    is_graduated: row.is_graduated,
                    is_nsfw: row.is_nsfw,
                    twitter: None,
                    telegram: None,
                    website: None,
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
                amount: row.amount.normalized().to_plain_string(),
                total_amount: row.total_amount.normalized().to_plain_string(),
                created_at: row.created_at,
                transaction_hash: row.transaction_hash,
            })
            .collect();

        Ok(HypeRewardAddHistoryResponse {
            history,
            total_count,
        })
    }
}
