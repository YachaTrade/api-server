pub mod wallet;
pub mod x;
use std::{
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};

use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

use super::common::identifier::Identifier;

#[derive(Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub image_uri: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AccountResponse {
    pub account: Account,
}

#[derive(Deserialize, ToSchema)]
pub struct AccountParams {
    pub account_id: String,
    pub request_account_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestAccountIdParam {
    pub request_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MutualFriend {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub follower_count: i32,
    pub following_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Mutual {
    pub mutual_friends: Option<Vec<MutualFriend>>,
    pub mutual_friends_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Account {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub bio: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub mutual: Option<Mutual>,
}

impl Account {
    pub fn new(account_id: String) -> Self {
        //random_number 는 1~5까지의 숫자가 나와야함.
        let random_number = rand::thread_rng().gen_range(1..=5);
        let image_key = format!("DEFAULT_IMAGE_{}", random_number);
        let image_uri = env::var(&image_key).expect("DEFAULT_IMAGE must be set");
        Self {
            account_id: account_id.clone(),
            image_uri,
            nickname: account_id,
            bio: "".to_string(),
            follower_count: 0,
            following_count: 0,
            mutual: None,
        }
    }
}
#[derive(sqlx::FromRow)]
struct AccountRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    bio: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
}
#[derive(sqlx::FromRow)]
struct AccountMutualRow {
    account_id: String,
    nickname: String,
    image_uri: String,
    bio: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
    mutual_friends: Option<serde_json::Value>,
    mutual_friends_count: Option<i64>,
}
pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn upsert_account(&self, account: Account) -> Result<Account> {
        let start_time = Instant::now();

        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (account_id) 
            DO NOTHING
            RETURNING account_id, nickname, image_uri, bio, follower_count, following_count, NULL as x_handle, NULL as x_image_uri, NULL as is_blue_label
            "#,
        )
        .bind(&account.account_id)
        .bind(&account.image_uri)
        .bind(&account.nickname)
        .bind(&account.bio)
        .bind(account.follower_count)
        .bind(account.following_count)
        .fetch_optional(self.db.get_write_pool());

        let result = tokio::time::timeout(Duration::from_millis(1000), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Failed to upsert account. Reason: {:?}", err))?;

        let elapsed = start_time.elapsed();
        info!(
            "upsert_account(account_id: {}) completed in {:?}",
            account.account_id, elapsed
        );

        if elapsed > Duration::from_millis(100) {
            warn!(
                "upsert_account query slow performance: {:?} for account_id: {}",
                elapsed, account.account_id
            );
        }

        match result {
            Some(row) => {
                let account = Account {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                    bio: row.bio,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                    mutual: None,
                };
                Ok(account)
            }
            None => {
                // ON CONFLICT DO NOTHING으로 아무것도 반환되지 않았다면, 이미 존재하는 계정
                // 이 경우에만 get_account 호출 (하지만 이미 존재하므로 순환 호출 없음)
                let query = sqlx::query_as::<_, AccountRow>(
                    r#"
                   SELECT a.account_id,
                        a.nickname,
                        a.image_uri,
                        a.bio,
                        a.follower_count,
                        a.following_count,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label
                        FROM account a
                        LEFT JOIN account_x ax ON a.account_id = ax.account_id
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    WHERE a.account_id = $1
                    "#,
                )
                .bind(&account.account_id)
                .fetch_one(self.db.get_read_pool());
                let row = tokio::time::timeout(Duration::from_millis(1000), query)
                    .await
                    .map_err(|_| anyhow!("Query timeout after 1000ms"))?;
                let row = match row {
                    Ok(row) => row,
                    Err(_) => {
                        return Err(anyhow!("Account not found: {}", account.account_id));
                    }
                };
                let account = Account {
                    account_id: row.account_id,
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
                    bio: row.bio,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                    mutual: None,
                };
                Ok(account)
            }
        }
    }

    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<Account> {
        let start_time = Instant::now();
        // 1. UPDATE 구문 시작
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE account SET ");

        // 2. 업데이트할 필드를 동적으로 저장할 벡터
        let mut fields = vec![];

        // 3. 각 필드가 Some 값이면 fields에 (필드명, 값)을 push
        if let Some(image_uri) = image_uri {
            fields.push(("image_uri", image_uri));
        }

        if let Some(nickname) = nickname {
            fields.push(("nickname", nickname));
        }

        if let Some(bio) = bio {
            fields.push(("bio", bio));
        }

        // 4. 업데이트할 필드가 하나도 없다면 에러 처리 (또는 skip 로직 가능)
        if fields.is_empty() {
            return Err(anyhow!("No fields provided to update"));
        }

        // 5. 쿼리 빌드
        for (i, (field_name, field_value)) in fields.into_iter().enumerate() {
            if i > 0 {
                // 첫 필드가 아니라면 ,(콤마) 추가
                query_builder.push(", ");
            }
            // 예: field_name = $1
            query_builder.push(format!("{} = ", field_name));
            query_builder.push_bind(field_value);
        }

        // 7. 쿼리 실행
        query_builder
            .push(" WHERE account_id = ")
            .push_bind(address)
            // UPDATE 한 뒤 해당 컬럼들을 바로 반환
            .push(" RETURNING *");

        // 한 번에 쿼리 실행 & 바로 레코드 받아오기
        let query = query_builder.build();
        let update_query = query
            .try_map(|row: PgRow| {
                // 여기서 row에서 컬럼을 뽑아 Account 구조체로 매핑
                Ok(Account {
                    account_id: row.try_get("account_id")?,
                    image_uri: row.try_get("image_uri")?,
                    nickname: row.try_get("nickname")?,
                    bio: row.try_get("bio")?,
                    follower_count: row.try_get("follower_count")?,
                    following_count: row.try_get("following_count")?,
                    mutual: None,
                })
            })
            .fetch_one(self.db.get_write_pool()); // 풀에서 커넥션 얻기

        let updated_account = tokio::time::timeout(Duration::from_millis(1000), update_query)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?
            .map_err(|err| anyhow!("Fail update account. Reason: {err} address: {}", address))?;

        let elapsed = start_time.elapsed();
        info!(
            "update_account(account_id: {}) completed in {:?}",
            address, elapsed
        );

        Ok(updated_account)
    }

    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let start_time = Instant::now();

        // 캐시 키 생성
        let cache_key = cache_key!("account", account_id);

        // Single Flight Pattern 적용
        let account = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_account(account_id).await
        })
        .await?;

        let elapsed = start_time.elapsed();
        info!(
            "get_account(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(account)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Account> {
        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT a.account_id,
            a.nickname,
            a.image_uri,
            a.bio,
            a.follower_count,
            a.following_count,
            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
            ax.x_image_uri,
            ax.is_blue_label
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE a.account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_one(self.db.get_read_pool());

        let row = tokio::time::timeout(Duration::from_millis(1000), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 1000ms"))?;

        let row = match row {
            Ok(row) => row,
            Err(_) => {
                let account = Account::new(account_id.to_string());
                self.upsert_account(account.clone())
                    .await
                    .map_err(|err| anyhow!("Fail upsert account Reason :{err} address: {}", err))?;
                return Ok(account);
            }
        };
        let account = Account {
            account_id: row.account_id,
            nickname: match &row.x_handle {
                Some(handle) if !handle.is_empty() => handle.clone(),
                _ => row.nickname,
            },
            image_uri: match &row.x_image_uri {
                Some(img) if !img.is_empty() => img.clone(),
                _ => row.image_uri,
            },
            bio: row.bio,
            follower_count: row.follower_count,
            following_count: row.following_count,
            mutual: None,
        };

        Ok(account)
    }
}
