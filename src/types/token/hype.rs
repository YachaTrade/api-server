use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::types::common::CountRow;
use crate::types::common::info::{AccountInfoWithX, TokenInfoWithDescription, XInfo};
use crate::types::common::pagination::PaginationParams;
use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    types::common::info::AccountInfo,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

// 홀더 응답을 위한 구조체
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenHolderResponse {
    pub holders: Vec<AccountInfoWithX>,
    pub total_count: u64,
}

// 데이터베이스 쿼리 결과를 담을 구조체
#[derive(Debug, sqlx::FromRow)]
struct HypeTokenRecord {
    token_id: String,
    name: String,
    symbol: String,
    image_uri: String,
    description: Option<String>,
    creator_account_id: String,
    creator_nickname: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
    holder_count: Option<i64>,
    market_cap: Option<BigDecimal>,
    current_price: Option<BigDecimal>,
    day_ago_price: Option<BigDecimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeInfo {
    pub holder_count: u64,
    pub price_increate_rate: BigDecimal,
    pub market_cap: BigDecimal,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeToken {
    pub token_info: TokenInfoWithDescription,
    pub account_info: AccountInfoWithX,
    pub hype_info: HypeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HypeTokenResponse {
    pub tokens: Vec<HypeToken>,
    pub total_count: u64,
}
pub struct HypeTokenController {
    pub db: Arc<PostgresDatabase>,
}

impl HypeTokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        HypeTokenController { db }
    }

    pub async fn get_hype_token(&self, pagination: &PaginationParams) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = cache_key!("hype_token", pagination.page, pagination.limit);

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let pagination = pagination;
            async move {
                let controller = HypeTokenController::new(db);
                controller.fetch_hype_token(&pagination).await
            }
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_hype_token completed in {:?} for page: {}, limit: {}",
            elapsed, pagination.page, pagination.limit
        );
        Ok(response)
    }

    async fn fetch_hype_token(&self, pagination: &PaginationParams) -> Result<HypeTokenResponse> {
        info!("Get Hype Token start");
        // 현재 시간 타임스탬프 (초 단위) 구하기
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("SystemTime error: {}", e))?
            .as_secs() as i64;

        // 24시간 전 타임스탬프 계산
        let day_ago_timestamp = current_time - (24 * 60 * 60);

        // 1분 간격 차트 데이터 사용
        let interval_type = "1";

        // 페이지네이션 계산
        let offset = (pagination.page - 1) * pagination.limit;

        // 데이터 조회 Future - 최적화된 버전
        let records_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, HypeTokenRecord>(
                r#"
                SELECT 
                    h.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    t.description,
                    t.creator as creator_account_id,
                    a.nickname as creator_nickname,
                    a.image_uri as creator_image_uri,
                    a.follower_count as creator_follower_count, 
                    a.following_count as creator_following_count,
                    x.x_handle as x_handle,
                    x.x_image_uri as x_image_uri,
                    x.is_blue_label as is_blue_label,
                    -- holder_count 테이블 사용으로 최적화
                    COALESCE(thc.holder_count, 0) as holder_count,
                    -- 시가총액 계산 (가격 * 총 공급량)
                    COALESCE(m.price * t.total_supply, 0) as market_cap,
                    -- 현재 가격
                    m.price as current_price,
                    -- 24시간 전 가격 최적화 (LATERAL JOIN 사용)
                    chart_price.day_ago_price
                FROM hype_token h
                JOIN token t ON h.token_id = t.token_id
                JOIN market m ON h.token_id = m.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN token_holder_count thc ON h.token_id = thc.token_id
                -- LATERAL JOIN으로 account_x 최적화 - 필요한 레코드만 조회
                LEFT JOIN LATERAL (
                    SELECT 
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri, 
                        ax.is_blue_label 
                    FROM account_x ax
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    WHERE ax.account_id = a.account_id 
                    LIMIT 1
                ) x ON true
                -- 차트 가격 최적화 - COALESCE 패턴을 LATERAL JOIN으로 변경
                LEFT JOIN LATERAL (
                    SELECT 
                        COALESCE(
                            (SELECT close_price 
                             FROM chart c 
                             WHERE c.token_id = h.token_id 
                               AND c.interval_type = $1 
                               AND c.time_stamp <= $2 
                             ORDER BY c.time_stamp DESC 
                             LIMIT 1),
                            (SELECT close_price 
                             FROM chart c 
                             WHERE c.token_id = h.token_id 
                               AND c.interval_type = $1 
                             ORDER BY c.time_stamp ASC 
                             LIMIT 1)
                        ) as day_ago_price
                ) chart_price ON true
                ORDER BY m.price DESC NULLS LAST
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(interval_type)
            .bind(day_ago_timestamp)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        );

        // 총 개수 조회 Future - 캐싱 가능한 데이터, 필요한 경우 별도 테이블에 저장할 수 있음
        let total_count_future = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(*) as count
                FROM hype_token h
                "#,
            )
            .fetch_one(self.db.get_read_pool()),
        );

        // 두 쿼리를 병렬로 실행
        let (records_result, total_count_result) = tokio::join!(records_future, total_count_future);

        // 결과 처리
        let token_records = records_result.map_err(|_| anyhow!("Query timeout after 1000ms"))??;
        let total_count = total_count_result
            .map_err(|_| anyhow!("Query timeout after 1000ms"))??
            .count as u64;

        // 결과 매핑
        let tokens = token_records
            .into_par_iter()
            .map(|record| {
                // 가격 증가율 계산 (백분율)
                let price_increase_rate = match (&record.current_price, &record.day_ago_price) {
                    (Some(current_price), Some(day_ago_price)) => {
                        info!(
                            "token_id = {:?}, current_price: {:?}, day_ago_price: {:?}",
                            record.token_id, current_price, day_ago_price
                        );
                        if *day_ago_price == BigDecimal::from(0) {
                            BigDecimal::from(0)
                        } else {
                            // ((현재 가격 - 이전 가격) / 이전 가격) * 100
                            ((current_price.clone() - day_ago_price.clone())
                                / day_ago_price.clone())
                                * BigDecimal::from(100)
                        }
                    }
                    _ => BigDecimal::from(0),
                };

                HypeToken {
                    token_info: TokenInfoWithDescription {
                        token_id: record.token_id,
                        name: record.name,
                        symbol: record.symbol,
                        image_uri: record.image_uri,
                        description: record.description,
                    },
                    account_info: AccountInfoWithX {
                        account_info: AccountInfo {
                            account_id: record.creator_account_id,
                            nickname: record.creator_nickname,
                            image_uri: record.creator_image_uri,
                            follower_count: record.creator_follower_count,
                            following_count: record.creator_following_count,
                        },
                        x_info: match (record.x_handle, record.x_image_uri, record.is_blue_label) {
                            (Some(handle), Some(image_uri), Some(is_blue)) => Some(XInfo {
                                x_handle: handle,
                                x_image_uri: image_uri,
                                is_blue_label: is_blue,
                            }),
                            _ => None,
                        },
                    },
                    hype_info: HypeInfo {
                        holder_count: record.holder_count.unwrap_or_default() as u64,
                        price_increate_rate: price_increase_rate,
                        market_cap: record.market_cap.unwrap_or_default(),
                    },
                }
            })
            .collect::<Vec<HypeToken>>();

        Ok(HypeTokenResponse {
            tokens,
            total_count,
        })
    }
}
