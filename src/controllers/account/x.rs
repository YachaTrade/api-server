use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        account::x::{
            ConnectXRequest, ConnectedXAccountResponse, DisconnectedXAccountResponse,
            GetXHandleResponse,
        },
        common::{ExistsRow, info::XInfo},
    },
};

#[derive(FromRow)]
struct XAccountRow {
    account_id: String,
    x_handle: String,
    x_image_uri: String,
    is_blue_label: bool,
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
        let query = sqlx::query_as::<_, XAccountRow>(
            r#"
            INSERT INTO account_x (account_id, x_handle, x_image_uri, is_blue_label)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(&account_id)
        .bind(&req.x_handle)
        .bind(&req.x_image_uri)
        .bind(req.is_blue_label)
        .fetch_one(self.db.get_write_pool());

        let record = measure_postgres!("account_x.connect", query)
            .map_err(|err| anyhow!("Failed to connect x\n Reason :{err}"))?;

        let response = ConnectedXAccountResponse {
            account_id: record.account_id,
            x_handle: record.x_handle,
            x_image_uri: record.x_image_uri,
            is_blue_label: record.is_blue_label,
        };

        Ok(response)
    }

    pub async fn disconnect_x(
        &self,
        account_id: String,
        x_handle: String,
    ) -> Result<DisconnectedXAccountResponse> {
        let query = sqlx::query_as::<_, ExistsRow>(
            "SELECT EXISTS(SELECT 1 FROM account_x WHERE account_id = $1 AND x_handle = $2) as exists",
        )
        .bind(&account_id)
        .bind(&x_handle)
        .fetch_optional(self.db.get_read_pool());

        let exists = measure_postgres!("account_x.disconnect.exists", query)
            .map_err(|err| anyhow!("Failed to check if x handle exists\n Reason :{err}"))?;

        if exists.is_none() {
            return Err(anyhow!("X handle not found for this account"));
        }

        let query = sqlx::query(
            r#"
            WITH deleted_verified AS (
                DELETE FROM account_verified
                WHERE x_handle = $2
                RETURNING x_handle
            )
            DELETE FROM account_x
            WHERE account_id = $1 AND x_handle = $2
            "#,
        )
        .bind(&account_id)
        .bind(&x_handle)
        .execute(self.db.get_write_pool());

        measure_postgres!("account_x.disconnect", query)
            .map_err(|err| anyhow!("Failed to disconnect x\n Reason :{err}"))?;

        Ok(DisconnectedXAccountResponse {
            account_id,
            x_handle,
        })
    }

    pub async fn update_x(
        &self,
        account_id: String,
        x_image_uri: String,
    ) -> Result<GetXHandleResponse> {
        let query = sqlx::query_as::<_, XInfo>(
            r#"
            UPDATE account_x
            SET x_image_uri = $2
            WHERE account_id = $1
            RETURNING *
            "#,
        )
        .bind(&account_id)
        .bind(&x_image_uri)
        .fetch_one(self.db.get_write_pool());

        let x_info: XInfo = measure_postgres!("account_x.update", query)
            .map_err(|err| anyhow!("Failed to get x handle\n Reason :{err}"))?;

        Ok(GetXHandleResponse { account_id, x_info })
    }
}
