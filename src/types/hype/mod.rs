use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::types::common::info::TokenInfoWithCreatedAtAndDescription;

use crate::types::common::CountRow;

#[derive(Debug, sqlx::FromRow)]
struct AmountRow {
    amount: BigDecimal,
}
use crate::types::common::pagination::PaginationParams;
use crate::{
    db::postgres::PostgresDatabase,
    types::common::info::AccountInfo,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

// 데이터베이스 쿼리 결과를 담을 구조체
#[derive(Debug, sqlx::FromRow)]
struct HypeTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    image_uri: String,
    description: Option<String>,
    created_at: i64,
    creator_account_id: String,
    creator_nickname: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    vote: BigDecimal,
    holder_count: Option<i64>,
    market_cap: BigDecimal,
    reward_amount: Option<BigDecimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeInfo {
    pub vote: String,
    pub holder_count: u64,
    pub market_cap: String,
    pub reward_amount: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeToken {
    pub token_info: TokenInfoWithCreatedAtAndDescription,
    pub account_info: AccountInfo,
    pub hype_info: HypeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeTokenResponse {
    pub tokens: Vec<HypeToken>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointResponse {
    pub account_id: String,
    pub point: String,
    pub spend_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeEpochResponse {
    pub epoch: i64,
    pub start_at: i64,
    pub end_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteHistory {
    pub epoch: i64,
    pub token_info: TokenInfoWithCreatedAtAndDescription,
    pub vote_amount: String,
    pub total_vote_amount: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteHistoryResponse {
    pub history: Vec<HypeVoteHistory>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointRecord {
    pub epoch: i64,
    pub activity_type: String,
    pub amount: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypePointRecordResponse {
    pub history: Vec<HypePointRecord>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeReward {
    pub epoch: i64,
    pub token_info: TokenInfoWithCreatedAtAndDescription,
    pub amount: String,
    pub claimable: bool,
    pub proof: Vec<String>,
    pub transaction_hash: Option<String>,
    pub claim_at: Option<i64>,
    pub vote_amount: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeRewardHistoryResponse {
    pub history: Vec<HypeReward>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteRequest {
    pub token_id: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeVoteResponse {
    pub account_id: String,
    pub account_point: String,
    pub account_spend_point: String,
    pub token_vote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RewardAdd {
    pub epoch: i64,
    pub token_info: TokenInfoWithCreatedAtAndDescription,
    pub amount: String,
    pub total_amount: String,
    pub created_at: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeRewardAddHistoryResponse {
    pub history: Vec<RewardAdd>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AmountResponse {
    pub amount: String,
}

pub struct HypeController {
    pub db: Arc<PostgresDatabase>,
}

impl HypeController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        HypeController { db }
    }

    pub async fn get_hype_token(&self) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = "hype_token";

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_token().await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!("get_hype_token() completed in {:?}", elapsed);
        Ok(response)
    }

    async fn fetch_hype_token(&self) -> Result<HypeTokenResponse> {
        let rows_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, HypeTokenRow>(
                r#"
                SELECT 
                    h.vote,
                    -- token 정보
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    t.description,
                    t.total_supply,
                    t.created_at,
                    -- account 정보 (creator) - verified 우선 처리
                    a.account_id as creator_account_id,
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE a.nickname END as creator_nickname,
                    CASE WHEN av.x_handle IS NOT NULL THEN ax.x_image_uri ELSE a.image_uri END as creator_image_uri,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count,
                    -- holder_count 테이블 사용으로 최적화
                    COALESCE(thc.holder_count, 0) as holder_count,
                    -- market cap 정보
                    (m.price * t.total_supply) as market_cap,
                    -- reward 정보
                    r.amount as reward_amount
                FROM hype_token h
                -- token 정보 조인
                JOIN token t ON h.token_id = t.token_id
                -- creator account 정보 조인  
                JOIN account a ON t.creator = a.account_id
                -- account_x 정보 조인 (LEFT JOIN - 없을 수도 있음)
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                -- account_verified 정보 조인 (LEFT JOIN - verified 아닐 수도 있음)
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                -- market 정보 조인
                JOIN market m ON h.token_id = m.token_id
                LEFT JOIN token_holder_count thc ON h.token_id = thc.token_id
                -- reward 정보 조인 (같은 epoch, 같은 token)
                LEFT JOIN reward_pool r ON h.epoch = r.epoch AND h.token_id = r.token_id
                WHERE h.epoch = (SELECT MAX(epoch) FROM hype_token)
                ORDER BY h.vote DESC, market_cap DESC
                "#,
            )

            .fetch_all(self.db.get_read_pool())
        );

        // 총 개수 조회 Future - 캐싱 가능한 데이터, 필요한 경우 별도 테이블에 저장할 수 있음
        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(*) as count
                FROM hype_token h
                WHERE h.epoch = (SELECT MAX(epoch) FROM hype_token)
                "#,
            )
            .fetch_one(self.db.get_read_pool()),
        );

        // 두 쿼리를 병렬로 실행
        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        // 결과 처리
        let token_rows = rows_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .count as u64;

        // 결과 매핑
        let tokens = token_rows
            .into_par_iter()
            .map(|row| HypeToken {
                token_info: TokenInfoWithCreatedAtAndDescription {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    created_at: row.created_at,
                    description: row.description,
                },
                account_info: AccountInfo {
                    account_id: row.creator_account_id,
                    nickname: row.creator_nickname,
                    image_uri: row.creator_image_uri,
                    follower_count: row.creator_follower_count,
                    following_count: row.creator_following_count,
                },
                hype_info: HypeInfo {
                    vote: row.vote.to_plain_string(),
                    holder_count: row.holder_count.unwrap_or_default() as u64,
                    market_cap: row.market_cap.to_plain_string(),
                    reward_amount: row.reward_amount.unwrap_or_default().to_plain_string(),
                },
            })
            .collect::<Vec<HypeToken>>();

        Ok(HypeTokenResponse {
            tokens,
            total_count,
        })
    }

    pub async fn get_hype_point(&self, account_id: &str) -> Result<HypePointResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = format!("hype_point:{}", account_id);

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_point(&account_id).await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_point(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(response)
    }

    async fn fetch_hype_point(&self, account_id: &str) -> Result<HypePointResponse> {
        #[derive(sqlx::FromRow)]
        struct PointRow {
            account_id: String,
            point: i64,
            spend_point: i64,
        }

        let row = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, PointRow>(
                r#"
                SELECT 
                    account_id,
                    point,
                    spend_point
                FROM point
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_optional(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        match row {
            Some(row) => Ok(HypePointResponse {
                account_id: row.account_id,
                point: row.point.to_string(),
                spend_point: row.spend_point.to_string(),
            }),
            None => Ok(HypePointResponse {
                account_id: account_id.to_string(),
                point: "0".to_string(),
                spend_point: "0".to_string(),
            }),
        }
    }

    pub async fn get_hype_epoch(&self) -> Result<HypeEpochResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = "hype_epoch";

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_hype_epoch().await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!("get_hype_epoch() completed in {:?}", elapsed);
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
        let row = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, HypeEpochRow>(
                r#"
                SELECT 
                    epoch,
                    start_at,
                    end_at,
                    status
                FROM epoch 
                WHERE epoch = (SELECT MAX(epoch) FROM epoch)
                "#,
            )
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

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
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = format!(
            "hype_vote_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        // Single Flight Pattern 적용
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

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_vote_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
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
            vote: BigDecimal,
            total_vote_amount: BigDecimal,
            created_at: i64,
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
                t.created_at as token_created_at
            FROM vote_history vh
            JOIN token t ON vh.token_id = t.token_id
            WHERE vh.account_id = $1
            ORDER BY vh.created_at DESC
            LIMIT $2 OFFSET $3
            "#;

        let count_query = r#"
            SELECT COALESCE(total_count, 0) as count
            FROM account_vote_history_count
            WHERE account_id = $1
            "#;

        let rows_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, HypeVoteHistoryRow>(query)
                .bind(account_id)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool()),
        );

        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(count_query)
                .bind(account_id)
                .fetch_optional(self.db.get_read_pool()),
        );

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let vote_history_rows =
            rows_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .map(|row| row.count as u64)
            .unwrap_or(0);

        let history = vote_history_rows
            .into_iter()
            .map(|row| HypeVoteHistory {
                epoch: row.epoch,
                token_info: TokenInfoWithCreatedAtAndDescription {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    created_at: row.token_created_at,
                    description: None,
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
    ) -> Result<HypePointRecordResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = format!(
            "hype_point_history:{}:{}:{}",
            account_id, pagination_params.page, pagination_params.limit
        );

        // Single Flight Pattern 적용
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

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_point_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination_params.page, pagination_params.limit, elapsed
        );
        Ok(response)
    }

    async fn fetch_hype_point_history(
        &self,
        account_id: &str,
        pagination_params: &PaginationParams,
    ) -> Result<HypePointRecordResponse> {
        let offset = (pagination_params.page - 1) * pagination_params.limit;

        #[derive(sqlx::FromRow)]
        struct PointDistributionRow {
            epoch: i64,
            activity_type: String,
            amount: i64,
            created_at: i64,
        }

        let rows_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, PointDistributionRow>(
                r#"
                SELECT 
                    epoch,
                    activity_type,
                    amount,
                    created_at
                FROM point_distribution
                WHERE account_id = $1
                ORDER BY created_at DESC, amount DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination_params.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool()),
        );

        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(total_count, 0) as count
                FROM account_point_distribution_count
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_optional(self.db.get_read_pool()),
        );

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let point_rows = rows_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .map(|row| row.count as u64)
            .unwrap_or(0);

        let history = point_rows
            .into_iter()
            .map(|row| HypePointRecord {
                epoch: row.epoch,
                activity_type: row.activity_type,
                amount: row.amount.to_string(),
                created_at: row.created_at,
            })
            .collect();

        Ok(HypePointRecordResponse {
            history,
            total_count,
        })
    }

    pub async fn get_hype_reward_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HypeRewardHistoryResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = format!(
            "hype_reward_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        // Single Flight Pattern 적용
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

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_reward_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
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
            epoch: i64,
            amount: BigDecimal,
            status: String,
            proof: Vec<String>,
            transaction_hash: Option<String>,
            claim_at: Option<i64>,
            vote_amount: BigDecimal,
            created_at: i64,
        }

        let rows_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, HypeRewardRow>(
                r#"
                SELECT 
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    t.created_at as token_created_at,
                    r.epoch,
                    r.vote_amount,
                    r.amount,
                    r.status,
                    r.proof,
                    r.transaction_hash,
                    r.claim_at,
                    r.created_at
                FROM reward r
                JOIN token t ON r.token_id = t.token_id
                WHERE r.account_id = $1
                ORDER BY r.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool()),
        );

        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(*) as count
                FROM reward
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool()),
        );

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let reward_rows = rows_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .count as u64;

        let history = reward_rows
            .into_iter()
            .map(|row| HypeReward {
                epoch: row.epoch,
                token_info: TokenInfoWithCreatedAtAndDescription {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    created_at: row.token_created_at,
                    description: None,
                },
                amount: row.amount.to_string(),
                claimable: row.status == "AWAITING",
                proof: row.proof,
                transaction_hash: row.transaction_hash,
                claim_at: row.claim_at,
                vote_amount: row.vote_amount.to_string(),
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
        let start_time = Instant::now();
        let amount = payload
            .amount
            .parse::<i64>()
            .map_err(|_| anyhow!("Invalid amount format"))?;

        #[derive(sqlx::FromRow)]
        struct VoteResult {
            new_point: i64,
            new_spend_point: i64,
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
                WHERE (SELECT point FROM point WHERE account_id = $1) >= $3
                RETURNING id
            ),
            point_update AS (
                UPDATE point 
                SET point = point - $3, spend_point = spend_point + $3
                WHERE account_id = $1 AND point >= $3
                AND EXISTS (SELECT 1 FROM vote_history_insert)
                RETURNING point as new_point, spend_point as new_spend_point
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
                (SELECT new_point FROM point_update) as new_point,
                (SELECT new_spend_point FROM point_update) as new_spend_point,
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

        let elapsed = start_time.elapsed();
        info!(
            "vote(account_id: {}, token_id: {}, amount: {}) completed in {:?}",
            account_id, payload.token_id, amount, elapsed
        );

        Ok(HypeVoteResponse {
            account_id: account_id.to_string(),
            account_point: vote_result.new_point.to_string(),
            account_spend_point: vote_result.new_spend_point.to_string(),
            token_vote: vote_result.new_vote.to_string(),
        })
    }

    pub async fn get_total_spend_point(&self) -> Result<AmountResponse> {
        let cache_key = "get_total_spend_point";

        with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            async move {
                let controller = HypeController::new(db);
                controller.fetch_total_spend_point().await
            }
        })
        .await
    }

    async fn fetch_total_spend_point(&self) -> Result<AmountResponse> {
        let start_time = Instant::now();

        let query = sqlx::query_as::<_, AmountRow>(
            "SELECT spend_point::NUMERIC as amount FROM total_spent_point WHERE id = 1",
        )
        .fetch_one(self.db.get_read_pool());

        let result = tokio::time::timeout(Duration::from_millis(1000), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to get total spend point\n Reason: {err}"))?;

        let elapsed = start_time.elapsed();
        info!("fetch_total_spend_point() completed in {:?}", elapsed);

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
        let start_time = Instant::now();

        // Get WMON balance from blockchain and buyback amount from database in parallel
        let (wmon_balance_result, buyback_sum_result) =
            tokio::join!(self.get_wmon_balance(), self.get_buyback_amount_sum());

        // Use 0 if any operation fails
        let wmon_balance = wmon_balance_result.unwrap_or_else(|e| {
            tracing::error!("Failed to get WMON balance: {}", e);
            BigDecimal::from(0)
        });
        let buyback_sum = buyback_sum_result.unwrap_or_else(|e| {
            tracing::error!("Failed to get buyback amount sum: {}", e);
            BigDecimal::from(0)
        });

        // Add WMON balance and buyback amount sum
        let total_amount = wmon_balance + buyback_sum;

        let elapsed = start_time.elapsed();
        info!("fetch_community_treasury() completed in {:?}", elapsed);

        Ok(AmountResponse {
            amount: total_amount.to_string(),
        })
    }

    async fn get_buyback_amount_sum(&self) -> Result<BigDecimal> {
        let start_time = Instant::now();

        let query = sqlx::query_as::<_, AmountRow>(
            "SELECT COALESCE(SUM(amount), 0) as amount FROM buyback_amount",
        )
        .fetch_one(self.db.get_read_pool());

        let result = tokio::time::timeout(Duration::from_millis(1000), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to get buyback amount sum\n Reason: {err}"))?;

        let elapsed = start_time.elapsed();
        info!("get_buyback_amount_sum() completed in {:?}", elapsed);

        Ok(result.amount)
    }

    async fn get_wmon_balance(&self) -> Result<bigdecimal::BigDecimal> {
        use crate::config::{COMMUNITY_TREASURY, RPC_URL, WMON};
        use alloy::primitives::{Address, U256};
        use alloy::providers::ProviderBuilder;
        use alloy::sol;

        // IERC20 interface definition
        sol! {
            #[allow(missing_docs)]
            #[sol(rpc)]
            interface IERC20 {
                function balanceOf(address account) external view returns (uint256);
            }
        }

        // Parse RPC URL
        let rpc_url: url::Url = RPC_URL
            .parse()
            .map_err(|e| anyhow!("Invalid RPC_URL: {}", e))?;

        // Initialize provider
        let provider = ProviderBuilder::new().connect_http(rpc_url);

        // Parse addresses
        let wmon_address: Address = WMON
            .parse()
            .map_err(|e| anyhow!("Invalid WMON address: {}", e))?;
        let treasury_address: Address = COMMUNITY_TREASURY
            .parse()
            .map_err(|e| anyhow!("Invalid COMMUNITY_TREASURY address: {}", e))?;

        // Create contract instance
        let contract = IERC20::new(wmon_address, provider);

        // Call balanceOf
        let balance_result = contract.balanceOf(treasury_address).call().await?;
        let balance: U256 = balance_result;

        // Convert U256 to BigDecimal
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
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = format!(
            "hype_reward_add_history:{}:{}:{}",
            account_id, pagination.page, pagination.limit
        );

        // Single Flight Pattern 적용
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

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_reward_add_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
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
            token_created_at: i64,
            amount: BigDecimal,
            total_amount: BigDecimal,
            transaction_hash: String,
            created_at: i64,
        }

        let rows_future = tokio::time::timeout(
            Duration::from_millis(1000),
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
                    t.created_at as token_created_at
                FROM reward_add_history rah
                JOIN token t ON rah.token_id = t.token_id
                WHERE rah.account_id = $1
                ORDER BY rah.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool()),
        );

        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(total_count, 0) as count
                FROM reward_add_history_count
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_optional(self.db.get_read_pool()),
        );

        let (rows_result, total_count_result) = tokio::join!(rows_future, total_count_future);

        let reward_add_rows = rows_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .map(|row| row.count as u64)
            .unwrap_or(0);

        let history = reward_add_rows
            .into_iter()
            .map(|row| RewardAdd {
                epoch: row.epoch,
                token_info: TokenInfoWithCreatedAtAndDescription {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                    created_at: row.token_created_at,
                    description: None,
                },
                amount: row.amount.to_string(),
                total_amount: row.total_amount.to_string(),
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
