pub mod wallet;
pub mod x;
use std::{env, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};

use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, Postgres, QueryBuilder, Row};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MutualFriend {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub follower_count: i32,
    pub following_count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Mutual {
    pub mutual_friends: Option<Vec<MutualFriend>>,
    pub mutual_friends_count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
        let query = sqlx::query!(
            r#"
            INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (account_id) 
            DO NOTHING
            RETURNING *
            "#,
            account.account_id,
            account.image_uri,
            account.nickname,
            account.bio,
            account.follower_count,
            account.following_count,
        )
        .fetch_optional(self.db.get_write_pool());
        
        tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to upsert account. Reason: {:?}", err))?;

        Ok(self.get_account(&account.account_id).await?)
    }

    pub async fn update_account(
        &self,
        address: &str,
        image_uri: Option<String>,
        nickname: Option<String>,
        bio: Option<String>,
    ) -> Result<Account> {
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
            
        let updated_account = tokio::time::timeout(Duration::from_millis(500), update_query)
            .await
            .map_err(|_| anyhow!("Query timeout after 500ms"))?
            .map_err(|err| anyhow!("Fail update account. Reason: {err} address: {}", address))?;

        Ok(updated_account)
    }

    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let query = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT a.account_id,
            a.nickname,
            a.image_uri,
            a.bio,
            a.follower_count,
            a.following_count,
            ax.x_handle,
            ax.x_image_uri,
            ax.is_blue_label
            FROM account a
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            WHERE LOWER(a.account_id) = LOWER($1)
            "#,
        )
        .bind(account_id)
        .fetch_one(self.db.get_read_pool());
        
        let row = tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Fail get account Reason :{err} address: {}", err))?;

        Ok(Account {
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
        })
    }

    pub async fn get_account_with_mutual(
        &self,
        identifier: &Identifier,
        request_account_id: Option<String>,
    ) -> Result<Account> {
        let id_type = match identifier {
            Identifier::Address(_) => "account_id",
            Identifier::Nickname(_) => "nickname",
        };

        let id_value = match identifier {
            Identifier::Address(addr) => addr,
            Identifier::Nickname(nick) => nick,
        };

        let query = sqlx::query_as::<_, AccountMutualRow>(
            r#"
            WITH target_account AS (
                SELECT 
                    a.account_id,           -- 여기에 a. 접두사 추가
                    a.nickname,             -- 모든 칼럼에 테이블 접두사 붙이기
                    a.image_uri,
                    a.bio,
                    a.follower_count,
                    a.following_count,
                    ax.x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label
                FROM account a
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                WHERE CASE 
                    WHEN $1 = 'account_id' THEN a.account_id = $2
                    ELSE a.nickname = $2
                END
            ),
            mutual_friends AS (
                SELECT 
                    a.account_id,           -- 여기도 a. 접두사 추가
                    a.nickname,
                    a.image_uri,
                    a.follower_count,
                    a.following_count,
                    ax.x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label,
                    COUNT(*) OVER() as total_count
                FROM follow f
                JOIN follow f_other ON f.following_id = f_other.following_id
                JOIN account a ON f.following_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                WHERE f.follower_id = $2 
                AND f_other.follower_id = $3
                LIMIT 3
            )
            SELECT 
                ta.account_id,              -- 여기도 ta. 접두사 추가
                ta.nickname,
                ta.image_uri,
                ta.bio,
                ta.follower_count,
                ta.following_count,
                ta.x_handle,
                ta.x_image_uri,
                ta.is_blue_label,
                COALESCE(
                    jsonb_agg(
                        jsonb_build_object(
                            'account_id', mf.account_id,    -- 모든 필드에 테이블 접두사 사용
                            'nickname', CASE 
                                WHEN mf.x_handle IS NOT NULL AND mf.x_handle != '' 
                                THEN mf.x_handle 
                                ELSE mf.nickname 
                            END,
                            'image_uri', CASE 
                                WHEN mf.x_image_uri IS NOT NULL AND mf.x_image_uri != '' 
                                THEN mf.x_image_uri 
                                ELSE mf.image_uri 
                            END,
                            'follower_count', mf.follower_count,
                            'following_count', mf.following_count
                        )
                    ) FILTER (WHERE mf.account_id IS NOT NULL),
                    NULL
                ) as mutual_friends,
                COALESCE(MAX(mf.total_count), 0) as mutual_friends_count
            FROM target_account ta
            LEFT JOIN mutual_friends mf ON true
            GROUP BY 
                ta.account_id,
                ta.nickname,
                ta.image_uri,
                ta.bio,
                ta.follower_count,
                ta.following_count,
                ta.x_handle,
                ta.x_image_uri,
                ta.is_blue_label
            "#,
        )
        .bind(id_type)
        .bind(id_value)
        .bind(&request_account_id)
        .fetch_one(self.db.get_read_pool());
        
        let result = tokio::time::timeout(Duration::from_millis(500), query)
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))?
        .map_err(|err| anyhow!("Failed to get account with mutual. Reason: {:?}", err))?;

        let mutual = if request_account_id.is_some() {
            Some(Mutual {
                mutual_friends: result
                    .mutual_friends
                    .map(|friends| serde_json::from_value(friends).unwrap_or_else(|_| vec![])),
                mutual_friends_count: result.mutual_friends_count.unwrap_or(0) as i32,
            })
        } else {
            None
        };

        Ok(Account {
            account_id: result.account_id,
            nickname: match &result.x_handle {
                Some(handle) if !handle.is_empty() => handle.clone(),
                _ => result.nickname,
            },
            image_uri: match &result.x_image_uri {
                Some(img) if !img.is_empty() => img.clone(),
                _ => result.image_uri,
            },
            bio: result.bio,
            follower_count: result.follower_count,
            following_count: result.following_count,
            mutual,
        })
    }
}
