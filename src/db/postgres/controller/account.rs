use std::sync::Arc;

use crate::db::postgres::{model::Account, PostgresDatabase};

use anyhow::{anyhow, Result};
use sqlx::{Postgres, QueryBuilder};
use tracing::info;

pub struct AccountController {
    pub db: Arc<PostgresDatabase>,
}

impl AccountController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        AccountController { db }
    }

    pub async fn upsert_account(&self, account: Account) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count, like_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (account_id) 
            DO UPDATE SET
                image_uri = $2,
                nickname = $3,
                bio = $4,
                follower_count = $5,
                following_count = $6,
                like_count = $7
                RETURNING *
            "#,
            account.account_id,
            account.image_uri,
            account.nickname,
            account.bio,
            account.follower_count,
            account.following_count,
            account.like_count,
        )
        .fetch_one(self.db.get_write_pool())
        .await
        .map_err(|err| anyhow!("Fail upsert account Reason :{:?}", err))?;
        Ok(account)
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

        // 6. WHERE 구문 추가
        query_builder.push(" WHERE account_id = ");
        query_builder.push_bind(address);

        // 7. 쿼리 실행
        let query = query_builder.build();
        query.execute(self.db.get_write_pool()).await?;

        // 8. 업데이트된 계정 조회 후 반환
        let updated_account = sqlx::query_as!(
            Account,
            r#"
                SELECT 
                    account_id,
                    image_uri, 
                    nickname,
                    bio,
                    follower_count,
                    following_count,
                    like_count
                FROM account
                WHERE LOWER(account_id) = LOWER($1)
            "#,
            address
        )
        .fetch_one(self.db.get_write_pool())
        .await
        .map_err(|err| anyhow!("Fail update account. Reason: {err} address: {}", err))?;
        info!("Updated account: {:#?}", updated_account);
        Ok(updated_account)
    }
    pub async fn get_account(&self, account_id: &str) -> Result<Account> {
        let account = sqlx::query_as!(
            Account,
            r#"
            SELECT *
            FROM account
            WHERE LOWER(account_id) = LOWER($1)
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow!("Fail get account Reason :{err} address: {}", err))?;
        Ok(account)
    }
}
