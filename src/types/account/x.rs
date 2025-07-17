use std::{sync::Arc, time::{Duration, Instant}};

use crate::{db::postgres::PostgresDatabase, types::common::info::XInfo};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use tracing::info;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConnectXRequest {
    pub x_handle: String,
    pub x_image_uri: String,
    pub is_blue_label: bool,
}
impl ConnectXRequest {
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        // x_handle 검증
        if !self.x_handle.starts_with('@') {
            return Err(anyhow!("X handle must start with @"));
        }

        if self.x_handle.len() > 15 {
            return Err(anyhow!("X handle must be 15 characters or less"));
        }

        // x_image_uri 검증
        if !url::Url::parse(&self.x_image_uri).is_ok() {
            return Err(anyhow!("Invalid image URI format"));
        }

        Ok(())
    }
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct DisconnectXRequest {
    pub x_handle: String,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectedXAccountResponse {
    pub account_id: String,
    pub x_handle: String,
    pub x_image_uri: String,
    pub is_blue_label: bool,
}
#[derive(Serialize, ToSchema)]
pub struct DisconnectedXAccountResponse {
    pub account_id: String,
    pub x_handle: String,
}

#[derive(Serialize, ToSchema)]
pub struct GetXHandleResponse {
    pub account_id: String,
    pub x_info: XInfo,
}

pub struct AccountXController {
    db: Arc<PostgresDatabase>,
}

impl AccountXController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountXController { db }
    }
    pub async fn connect_x(
        &self,
        account_id: &str,
        req: ConnectXRequest,
    ) -> Result<ConnectedXAccountResponse> {
        let start_time = Instant::now();
        
        let query = sqlx::query!(
            r#"
            INSERT INTO account_x (account_id, x_handle, x_image_uri, is_blue_label)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
            account_id,
            req.x_handle,
            req.x_image_uri,
            req.is_blue_label
        )
        .fetch_one(self.db.get_write_pool());
        
        let record = tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to connect x\n Reason :{err}"))?;

        let response = ConnectedXAccountResponse {
            account_id: record.account_id,
            x_handle: record.x_handle,
            x_image_uri: record.x_image_uri,
            is_blue_label: record.is_blue_label,
        };

        let elapsed = start_time.elapsed();
        info!("connect_x completed in {:?} for account_id: {}", elapsed, account_id);

        Ok(response)
    }

    pub async fn disconnect_x(
        &self,
        account_id: String,
        x_handle: String,
    ) -> Result<DisconnectedXAccountResponse> {
        let start_time = Instant::now();
        
        // 먼저 해당 X 핸들이 존재하는지 확인
        let query = sqlx::query!(
            "SELECT 1 as exists FROM account_x WHERE account_id = $1 AND x_handle = $2",
            account_id,
            x_handle
        )
        .fetch_optional(self.db.get_read_pool());
        
        let exists = tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to check if x handle exists\n Reason :{err}"))?;

        // 존재하지 않으면 NotFound 오류 반환
        if exists.is_none() {
            return Err(anyhow!("X handle not found for this account"));
        }

        // 존재하면 삭제 진행
        let query = sqlx::query!(
            r#"
            DELETE FROM account_x
            WHERE account_id = $1 AND x_handle = $2
            "#,
            account_id,
            x_handle
        )
        .execute(self.db.get_write_pool());
        
        tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to disconnect x\n Reason :{err}"))?;

        let elapsed = start_time.elapsed();
        info!("disconnect_x completed in {:?} for account_id: {}", elapsed, account_id);

        Ok(DisconnectedXAccountResponse {
            account_id,
            x_handle,
        })
    }

    pub async fn get_x_handle(&self, account_id: String) -> Result<GetXHandleResponse> {
        let start_time = Instant::now();
        
        let query = sqlx::query_as!(
            XInfo,
            r#"
            SELECT x_handle, x_image_uri, is_blue_label
            FROM account_x
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool());
        
        let x_info: XInfo = tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to get x handle\n Reason :{err}"))?;

        let elapsed = start_time.elapsed();
        info!("get_x_handle completed in {:?} for account_id: {}", elapsed, account_id);

        Ok(GetXHandleResponse { account_id, x_info })
    }
}
