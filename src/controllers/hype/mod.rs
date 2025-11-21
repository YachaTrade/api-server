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
            info::{AccountInfo, TokenInfo},
            pagination::PaginationParams,
        },
        hype::{
            AmountResponse, HypeEpochResponse, HypeInfo, HypePointResponse, HypeReward,
            HypeRewardAddHistoryResponse, HypeRewardHistoryResponse, HypeToken, HypeTokenResponse,
            HypeVoteHistory, HypeVoteHistoryResponse, HypeVoteRequest, HypeVoteResponse, RewardAdd,
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
    total_supply: BigDecimal,
    created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    vote: BigDecimal,
    holder_count: i64,
    market_cap: BigDecimal,
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
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETE' ORDER BY epoch DESC LIMIT 1)
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
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETE' ORDER BY epoch DESC LIMIT 1)
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
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
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                    WHERE h.epoch = COALESCE(
                        (SELECT epoch FROM epoch WHERE status = 'ACTIVE' LIMIT 1),
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETE' ORDER BY epoch DESC LIMIT 1)
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
                        (SELECT epoch FROM epoch WHERE status = 'COMPLETE' ORDER BY epoch DESC LIMIT 1)
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
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
                        t.total_supply,
                        t.created_at,
                        t.creator,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        t.token_holder_count as holder_count,
                        (m.price * t.total_supply) as market_cap,
                        r.amount as reward_amount
                    FROM hype_token h
                    JOIN token t ON h.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
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
                },
                hype_info: HypeInfo {
                    vote: row.vote.normalized().to_plain_string(),
                    holder_count: row.holder_count as u64,
                    market_cap: row.market_cap.normalized().to_plain_string(),
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
                    .fetch_hype_vote_history(&account_id, &pagination)
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
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            token_created_at: i64,
            is_graduated: bool,
            is_nsfw: bool,
            vote: BigDecimal,
            total_vote_amount: BigDecimal,
            created_at: i64,
            creator: String,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
        }

        let query = r#"
            SELECT
                vh.epoch,
                vh.token_id,
                vh.vote,
                vh.total_vote_amount,
                vh.created_at,
                t.name,
                t.symbol,
                t.image_uri,
                t.created_at as token_created_at,
                t.creator,
                t.is_nsfw,
                t.is_graduated,
                COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                a.bio as creator_bio,
                COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri
            FROM vote_history vh
            JOIN token t ON vh.token_id = t.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            WHERE vh.account_id = $1
            ORDER BY vh.created_at DESC
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
                },
                vote_amount: row.vote.to_string(),
                total_vote_amount: row.total_vote_amount.to_string(),
                created_at: row.created_at,
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
                    .fetch_hype_point_history(&account_id, &pagination)
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

        let history = point_rows
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
            history,
            total_count,
        })
    }

    pub async fn get_hype_reward_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeRewardHistoryResponse> {
        let cache_key = format!(
            "hype_reward_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();

            async move {
                let controller = HypeController::new(db);
                controller
                    .fetch_hype_reward_history(&account_id, &pagination)
                    .await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_hype_reward_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeRewardHistoryResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct HypeRewardRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            token_created_at: i64,
            is_graduated: bool,
            is_nsfw: bool,
            epoch: i64,
            amount: BigDecimal,
            status: String,
            proof: Vec<String>,
            transaction_hash: Option<String>,
            claim_at: Option<i64>,
            vote_amount: BigDecimal,
            created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            creator_follower_count: i32,
            creator_following_count: i32,
        }

        let rows_future = async {
            measure_postgres!(
                "hype.fetch_hype_reward_history.rows",
                sqlx::query_as::<_, HypeRewardRow>(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri,
                        t.created_at as token_created_at,
                        t.is_graduated,
                        t.is_nsfw,
                        r.epoch,
                        r.vote_amount,
                        r.amount,
                        r.status,
                        r.proof,
                        r.transaction_hash,
                        r.claim_at,
                        r.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count
                    FROM reward r
                    JOIN token t ON r.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    WHERE r.account_id = $1 
                    ORDER BY r.created_at DESC
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
                "hype.fetch_hype_reward_history.count",
                sqlx::query_as::<_, CountRow>(
                    r#"
                    SELECT COUNT(*) as count
                    FROM reward
                    WHERE account_id = $1
                    "#,
                )
                .bind(account_id)
                .fetch_one(self.db.get_read_pool())
            )
        };

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let reward_rows =
            rows_result.map_err(|err| anyhow!("Failed to fetch hype reward history: {}", err))?;
        let total_count = total_count_result
            .map_err(|err| anyhow!("Failed to fetch hype reward history count: {}", err))?
            .count as u64;

        let history = reward_rows
            .into_iter()
            .map(|row| HypeReward {
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
                },
                amount: row.amount.normalized().to_plain_string(),
                claimable: row.status == "AWAITING",
                proof: row.proof,
                transaction_hash: row.transaction_hash,
                claim_at: row.claim_at,
                vote_amount: row.vote_amount.normalized().to_plain_string(),
                created_at: row.created_at,
            })
            .collect();

        Ok(HypeRewardHistoryResponse {
            history,
            total_count,
        })
    }

    pub async fn vote(
        &self,
        account_id: &str,
        payload: &HypeVoteRequest,
    ) -> Result<HypeVoteResponse> {
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

        let result = sqlx::query_as::<_, VoteResult>(
            r#"
            WITH vote_history_insert AS (
                INSERT INTO vote_history (epoch, token_id, account_id, vote, total_vote_amount)
                SELECT
                    (SELECT epoch FROM epoch WHERE status = 'ACTIVE'),
                    $2,
                    $1,
                    $3,
                    (SELECT vote + $3 FROM hype_token
                     WHERE epoch = (SELECT epoch FROM epoch WHERE status = 'ACTIVE')
                     AND token_id = $2)
                WHERE (SELECT round_point FROM point WHERE account_id = $1) >= $3
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
                WHERE epoch = (SELECT epoch FROM epoch WHERE status = 'ACTIVE')
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
        let (wmon_balance_result, buyback_sum_result) =
            tokio::join!(self.get_wmon_balance(), self.get_buyback_amount_sum());

        let wmon_balance = wmon_balance_result.unwrap_or_else(|e| {
            tracing::error!("Failed to get WMON balance: {}", e);
            BigDecimal::from(0)
        });
        let buyback_sum = buyback_sum_result.unwrap_or_else(|e| {
            tracing::error!("Failed to get buyback amount sum: {}", e);
            BigDecimal::from(0)
        });

        let total_amount = wmon_balance + buyback_sum;

        Ok(AmountResponse {
            amount: total_amount.to_string(),
        })
    }

    async fn get_buyback_amount_sum(&self) -> Result<BigDecimal> {
        let query = sqlx::query_as::<_, AmountRow>(
            "SELECT COALESCE(SUM(amount), 0) as amount FROM total_buy_back",
        )
        .fetch_one(self.db.get_read_pool());

        let result = measure_postgres!("hype.total_buy_back", query)
            .map_err(|err| anyhow!("Failed to get buyback amount sum\n Reason: {err}"))?;
        Ok(result.amount)
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
                    .fetch_hype_reward_add_history(&account_id, &pagination)
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
            token_created_at: i64,
            amount: BigDecimal,
            total_amount: BigDecimal,
            transaction_hash: String,
            created_at: i64,
            creator: String,
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
