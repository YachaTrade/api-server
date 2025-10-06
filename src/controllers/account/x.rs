use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        account::{ConnectXRequest, UpdateXRequest},
        common::info::AccountInfo,
    },
};

pub struct AccountXController {
    db: Arc<PostgresDatabase>,
}

impl AccountXController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountXController { db }
    }

    pub async fn connect_x(&self, account_id: &str, req: ConnectXRequest) -> Result<AccountInfo> {
        // Insert X account
        let query = sqlx::query(
            r#"
            INSERT INTO account_x (account_id, x_handle, x_image_uri, is_blue_label)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(&account_id)
        .bind(&req.x_handle)
        .bind(&req.x_image_uri)
        .bind(req.is_blue_label)
        .execute(self.db.get_write_pool());

        measure_postgres!("account_x.connect", query)
            .map_err(|err| anyhow!("Failed to connect x\n Reason :{err}"))?;

        // Fetch account with X info
        let query = sqlx::query_as::<_, AccountInfo>(
            r#"
            SELECT
                a.account_id,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as nickname,
                COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                a.bio,
                a.follower_count,
                a.following_count
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE a.account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_one(self.db.get_read_pool());

        let account = measure_postgres!("account.get_with_x", query)?;
        Ok(account)
    }

    pub async fn disconnect_x(&self, account_id: String) -> Result<AccountInfo> {
        // Delete X account
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

        // Fetch account (X is now disconnected, so will use account values)
        let query = sqlx::query_as::<_, AccountInfo>(
            r#"
            SELECT
                a.account_id,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as nickname,
                COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                a.bio,
                a.follower_count,
                a.following_count
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE a.account_id = $1
            "#,
        )
        .bind(&account_id)
        .fetch_one(self.db.get_read_pool());

        let account = measure_postgres!("account.get", query)?;
        Ok(account)
    }

    pub async fn update_x(&self, account_id: String, req: UpdateXRequest) -> Result<AccountInfo> {
        // Update X image
        let query = sqlx::query(
            r#"
            UPDATE account_x
            SET x_image_uri = $2
            WHERE account_id = $1
            "#,
        )
        .bind(&account_id)
        .bind(&req.x_image_uri)
        .execute(self.db.get_write_pool());

        measure_postgres!("account_x.update", query)
            .map_err(|err| anyhow!("Failed to update x\n Reason :{err}"))?;

        // Fetch account with updated X info
        let query = sqlx::query_as::<_, AccountInfo>(
            r#"
            SELECT
                a.account_id,
                COALESCE(
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                    a.nickname
                ) as nickname,
                COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                a.bio,
                a.follower_count,
                a.following_count
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE a.account_id = $1
            "#,
        )
        .bind(&account_id)
        .fetch_one(self.db.get_read_pool());

        let account = measure_postgres!("account.get_with_x", query)?;
        Ok(account)
    }
}
