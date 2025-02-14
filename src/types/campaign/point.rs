use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::AccountInfo, pagination::PaginationParams},
};
use anyhow::Result;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct Point {
    pub account_info: AccountInfo,
    pub point: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TopPointResponse {
    pub points: Vec<Point>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountPointResponse {
    pub account_id: String,
    pub point: i64,
    pub rank: i64,
}

pub enum MissionType {
    ConnectWallet,
    CreateCoin,
    Trade,
    Follow,
    Posting,
    ReferrerCreate,
}

impl MissionType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "CONNECT_WALLET" => Some(Self::ConnectWallet),
            "CREATE_COIN" => Some(Self::CreateCoin),
            "TRADE" => Some(Self::Trade),
            "FOLLOW" => Some(Self::Follow),
            "POSTING" => Some(Self::Posting),
            "REFERRER_CREATE" => Some(Self::ReferrerCreate),
            _ => None,
        }
    }

    pub fn to_i64(&self) -> i64 {
        match self {
            Self::ConnectWallet => 50,
            Self::CreateCoin => 100,
            Self::Trade => 100,
            Self::Follow => 100,
            Self::Posting => 200,
            Self::ReferrerCreate => 1000,
        }
    }
}

pub struct PointController {
    pub db: Arc<PostgresDatabase>,
}

impl PointController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PointController { db }
    }
    pub async fn get_top_point(&self, pagination: PaginationParams) -> Result<TopPointResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let point_response = sqlx::query!(
            r#"
            SELECT
                p.account_id,
                p.point,
                a.nickname,
                a.image_uri,
                a.follower_count,
                a.following_count
            FROM point p
            JOIN account a ON p.account_id = a.account_id
            ORDER BY p.point DESC
            LIMIT $1
            OFFSET $2
            "#,
            pagination.limit as i64,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?
        .into_iter()
        .map(|row| Point {
            account_info: AccountInfo {
                account_id: row.account_id,
                image_uri: row.image_uri,
                nickname: row.nickname,
                follower_count: row.follower_count,
                following_count: row.following_count,
            },
            point: row.point,
        })
        .collect();

        Ok(TopPointResponse {
            points: point_response,
        })
    }

    pub async fn get_account_point_rank(&self, account_id: String) -> Result<AccountPointResponse> {
        let result = sqlx::query!(
            r#"
            SELECT rank, point
            FROM (
                SELECT account_id, point,
                       RANK() OVER (ORDER BY point DESC) as rank
                FROM point
            ) ranked
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_optional(self.db.get_read_pool())
        .await?;

        Ok(match result {
            Some(r) => AccountPointResponse {
                account_id,
                rank: r.rank.unwrap_or(1), // rank가 NULL이면 1로 설정
                point: r.point,
            },
            None => AccountPointResponse {
                account_id,
                rank: 1, // 결과가 없는 경우도 rank를 1로 설정
                point: 0,
            },
        })
    }

    pub async fn add_point_by_mission(
        &self,
        account_id: &str,
        mission_type: MissionType,
    ) -> Result<()> {
        let mut tx = self.db.write_pool.begin().await?;

        let mission_str = match mission_type {
            MissionType::ConnectWallet => "CONNECT_WALLET",
            MissionType::CreateCoin => "CREATE_COIN",
            MissionType::Trade => "TRADE",
            MissionType::Follow => "FOLLOW",
            MissionType::Posting => "POSTING",
            MissionType::ReferrerCreate => "REFERRER_CREATE",
        };

        // ON CONFLICT DO NOTHING 제거 - 중복 시 에러 발생
        sqlx::query!(
            r#"
            INSERT INTO mission_record(account_id, mission_type)
            VALUES ($1, $2)
            "#,
            account_id,
            mission_str
        )
        .execute(tx.as_mut())
        .await?; // 실패하면 여기서 에러 반환

        // mission_record 추가 성공한 경우에만 실행됨
        let points = mission_type.to_i64();

        sqlx::query!(
            r#"
            INSERT INTO point (account_id, point)
            VALUES ($1, $2)
            ON CONFLICT (account_id)
            DO UPDATE SET point = point.point + EXCLUDED.point
            "#,
            account_id,
            points
        )
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        Ok(())
    }
}
