use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        account::{
            Account,
            x::{
                ConnectXRequest, ConnectedXAccountResponse, DisconnectedXAccountResponse,
                GetXHandleResponse,
            },
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

#[derive(FromRow)]
struct AccountRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    bio: String,
    follower_count: i32,
    following_count: i32,
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

    pub async fn disconnect_x(&self, account_id: String) -> Result<DisconnectedXAccountResponse> {
        let query = sqlx::query(
            r#"
            DELETE FROM account_x
            WHERE account_id = $1 
            "#,
        )
        .bind(&account_id)
        .execute(self.db.get_write_pool());

        measure_postgres!("account_x.disconnect", query)
            .map_err(|err| anyhow!("Failed to disconnect x\n Reason :{err}"))?;
        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT account_id, nickname, image_uri, bio, 
        follower_count, following_count
            FROM account
            WHERE account_id = $1
            "#,
        )
        .bind(&account_id)
        .fetch_one(self.db.get_read_pool());

        let row = measure_postgres!("account.get", query)?;

        let account_info = Account {
            account_id: row.account_id,
            nickname: row.nickname,
            image_uri: row.image_uri,
            bio: row.bio,
            follower_count: row.follower_count,
            following_count: row.following_count,
            mutual: None, // 필요시 별도로 계산
        };
        Ok(DisconnectedXAccountResponse { account_info })
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
