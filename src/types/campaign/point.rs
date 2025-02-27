use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::AccountInfo, pagination::PaginationParams},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Point {
    pub account_info: AccountInfo,
    pub point: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TopPointResponse {
    pub points: Vec<Point>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AccountPointResponse {
    pub account_id: String,
    pub point: i64,
    pub rank: i64,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MissionType {
    ConnectWallet,
    CreateToken,
    Trade,
    Follow,
    Posting,
    ReferrerCreate,
}

impl MissionType {
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::ConnectWallet => 50,
            Self::CreateToken => 100,
            Self::Trade => 100,
            Self::Follow => 100,
            Self::Posting => 200,
            Self::ReferrerCreate => 1000,
        }
    }
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "CONNECT_WALLET" => Ok(Self::ConnectWallet),
            "CREATE_TOKEN" => Ok(Self::CreateToken),
            "TRADE" => Ok(Self::Trade),
            "FOLLOW" => Ok(Self::Follow),
            "POSTING" => Ok(Self::Posting),
            "REFERRER_CREATE" => Ok(Self::ReferrerCreate),
            _ => Err(anyhow::anyhow!("Invalid mission type")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MissionCompleteResponse {
    pub account_id: String,
    pub mission_type: MissionType,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MissionCompleteRequest {
    #[schema(example = "CONNECT_WALLET")]
    pub mission_type: MissionType,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MissionCompletedResponse {
    pub account_id: String,
    #[schema(example = json!([ "CONNECT_WALLET", "CREATE_COIN", "TRADE" ]))]
    pub mission_types: Vec<MissionType>,
}

pub struct PointController {
    pub db: Arc<PostgresDatabase>,
}

impl PointController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PointController { db }
    }
    pub async fn get_top_point(&self, pagination: &PaginationParams) -> Result<TopPointResponse> {
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

    pub async fn get_account_point_rank(&self, account_id: &str) -> Result<AccountPointResponse> {
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
                account_id: account_id.to_string(),
                rank: r.rank.unwrap_or(1), // rank가 NULL이면 1로 설정
                point: r.point,
            },
            None => AccountPointResponse {
                account_id: account_id.to_string(),
                rank: 1, // 결과가 없는 경우도 rank를 1로 설정
                point: 0,
            },
        })
    }

    pub async fn get_completed_missions(
        &self,
        account_id: &str,
    ) -> Result<MissionCompletedResponse> {
        let mission_types = match sqlx::query!(
            r#"
            SELECT mission_type
            FROM mission_record
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_all(self.db.get_read_pool())
        .await
        {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| MissionType::from_str(&row.mission_type).ok())
                .collect(),
            Err(_e) => {
                vec![] // 쿼리 실패 시 빈 배열 반환
            }
        };

        Ok(MissionCompletedResponse {
            account_id: account_id.to_string(),
            mission_types,
        })
    }
    pub async fn add_point_by_mission(
        &self,
        account_id: &str,
        mission_type: MissionType,
    ) -> Result<MissionCompleteResponse> {
        let mut tx = self.db.write_pool.begin().await?;

        let mission_str = match mission_type {
            MissionType::ConnectWallet => "CONNECT_WALLET",
            MissionType::CreateToken => "CREATE_TOKEN",
            MissionType::Trade => "TRADE",
            MissionType::Follow => "FOLLOW",
            MissionType::Posting => "POSTING",
            MissionType::ReferrerCreate => "REFERRER_CREATE",
        };

        // ON CONFLICT DO NOTHING 제거 - 중복 시 에러 발생
        let row = sqlx::query!(
            r#"
            INSERT INTO mission_record(account_id, mission_type)
            VALUES ($1, $2)
            RETURNING *
            "#,
            account_id,
            mission_str
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|err| anyhow::anyhow!("Already completed mission {err}"))?; // 실패하면 여기서 에러 반환

        let response = MissionCompleteResponse {
            account_id: row.account_id,
            mission_type: MissionType::from_str(&row.mission_type).unwrap(),
        };
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
        .await
        .map_err(|err| anyhow::anyhow!("Failed to add point by mission {err}"))?;

        tx.commit().await?;

        Ok(response)
    }
}
