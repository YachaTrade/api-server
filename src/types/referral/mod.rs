use std::sync::Arc;

use crate::db::postgres::PostgresDatabase;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckRegisterReferralResponse {
    pub account_id: String,
    pub is_registered: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterReferralRequest {
    pub parent_referral_code: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterReferralResponse {
    pub parent_account_id: String,
    pub child_account_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExistsReferralCodeResponse {
    pub account_id: String,
    pub exists: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MakeReferralCodeResponse {
    pub account_id: String,
    pub referral_code: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct GetReferralCodeResponse {
    pub account_id: String,
    pub referral_code: String,
}

/// Response for getting referral child count
#[derive(Debug, Serialize, ToSchema)]
pub struct GetReferralChildCountResponse {
    /// The account ID for which the child count was retrieved
    pub account_id: String,
    /// The number of referral children for this account
    pub child_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetInvitedCreateResponse {
    pub account_id: String,
    pub invited_by_create_count: i32,
}

pub struct ReferralController {
    pub db: Arc<PostgresDatabase>,
}

impl ReferralController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Checks if the given account has already registered under a parent referral
    ///
    /// # Arguments
    /// * `account_id` - The account ID to check for existing referral registration
    ///
    /// # Returns
    /// * `CheckRegisterReferralResponse` containing registration status
    pub async fn check_register_referral(
        &self,
        account_id: String,
    ) -> Result<CheckRegisterReferralResponse> {
        let record = sqlx::query!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM referral 
                WHERE child_account_id = $1
            ) AS is_registered
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow::anyhow!("Fail check referral Reason :{err}"))?;

        Ok(CheckRegisterReferralResponse {
            account_id,
            is_registered: record.is_registered.unwrap_or(false),
        })
    }

    /// Registers a new referral relationship between parent and child accounts
    ///
    /// # Arguments
    /// * `parent_account_id` - The account ID of the referrer (parent)
    /// * `child_account_id` - The account ID of the person being referred (child)
    ///
    /// # Returns
    /// * `RegisterReferralResponse` containing the registered relationship details
    pub async fn register_referral(
        &self,
        parent_account_id: &str,
        child_account_id: &str,
    ) -> Result<RegisterReferralResponse> {
        info!(
            "parent_account_id: {}, child_account_id: {}",
            parent_account_id, child_account_id
        );

        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|err| anyhow::anyhow!("Failed to start transaction: {}", err))?;

        sqlx::query!(
            r#"
            INSERT INTO referral (child_account_id, parent_account_id)
            VALUES ($1, $2)
            ON CONFLICT (child_account_id) DO NOTHING
            "#,
            child_account_id,
            parent_account_id
        )
        .execute(tx.as_mut())
        .await
        .map_err(|err| anyhow::anyhow!("Fail register referral Reason :{err}"))?;

        sqlx::query!(
            r#"
            UPDATE referral_code
            SET invite_count = invite_count + 1
            WHERE account_id = $1
            "#,
            parent_account_id
        )
        .execute(tx.as_mut())
        .await
        .map_err(|err| anyhow::anyhow!("Failed to update invite count: {}", err))?;

        tx.commit()
            .await
            .map_err(|err| anyhow::anyhow!("Failed to commit transaction: {}", err))?;

        Ok(RegisterReferralResponse {
            parent_account_id: parent_account_id.to_string(),
            child_account_id: child_account_id.to_string(),
        })
    }

    /// Retrieves the account ID associated with a given referral code
    ///
    /// # Arguments
    /// * `referral_code` - The referral code to look up
    ///
    /// # Returns
    /// * `String` containing the account ID if found
    /// * `Error` if no account is found for the given referral code
    pub async fn get_referral_account_id(&self, referral_code: &str) -> Result<String> {
        let record = sqlx::query!(
            r#"
            SELECT account_id
            FROM referral_code
            WHERE referral_code = $1
            "#,
            referral_code
        )
        .fetch_optional(self.db.get_read_pool())
        .await?;

        match record {
            Some(row) => Ok(row.account_id),
            None => Err(anyhow::anyhow!(
                "No account found for the given referral code"
            )),
        }
    }

    pub async fn get_referral_code(&self, account_id: &str) -> Result<GetReferralCodeResponse> {
        let record = sqlx::query!(
            r#"
            SELECT referral_code
            FROM referral_code
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow::anyhow!("Fail get referral code Reason :{err}"))?;

        Ok(GetReferralCodeResponse {
            account_id: account_id.to_string(),
            referral_code: record.referral_code,
        })
    }

    /// Checks if the given account has already generated a referral code
    ///
    /// # Arguments
    /// * `account_id` - The account ID to check for existing referral code
    ///
    /// # Returns
    /// * `ExistsReferralCodeResponse` containing whether the account has a referral code
    pub async fn existing_referral_code(
        &self,
        account_id: &str,
    ) -> Result<ExistsReferralCodeResponse> {
        let record = sqlx::query!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM referral_code 
                WHERE account_id = $1
            ) AS exists
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow::anyhow!("Fail check referral code Reason :{err}"))?;

        Ok(ExistsReferralCodeResponse {
            account_id: account_id.to_string(),
            exists: record.exists.unwrap_or(false),
        })
    }

    /// Generates a new unique referral code for the given account
    ///
    /// # Arguments
    /// * `account_id` - The account ID to generate a referral code for
    ///
    /// # Returns
    /// * `MakeReferralCodeResponse` containing the new referral code
    /// * `Error` if unable to generate a unique code after maximum attempts
    ///
    /// # Implementation Details
    /// - Makes multiple attempts to generate a unique code
    /// - Uses a retry mechanism to handle potential collisions
    /// - Maximum 10 attempts to generate a unique code
    pub async fn make_referral_code(&self, account_id: &str) -> Result<MakeReferralCodeResponse> {
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 10;

        while attempts < MAX_ATTEMPTS {
            let code = Self::generate_random_code(6);
            let result = sqlx::query!(
                r#"
                INSERT INTO referral_code (account_id, referral_code)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                RETURNING referral_code
                "#,
                account_id,
                code
            )
            .fetch_optional(self.db.get_write_pool())
            .await
            .map_err(|err| anyhow::anyhow!("Failed to insert referral code: {}", err))?;

            if let Some(record) = result {
                return Ok(MakeReferralCodeResponse {
                    account_id: account_id.to_string(),
                    referral_code: record.referral_code,
                });
            }

            attempts += 1;
        }

        Err(anyhow::anyhow!(
            "Failed to generate unique referral code after {} attempts",
            MAX_ATTEMPTS
        ))
    }

    pub async fn get_referral_child_count(
        &self,
        account_id: &str,
    ) -> Result<GetReferralChildCountResponse> {
        let record = sqlx::query!(
            r#"
            SELECT COUNT(*) AS count
            FROM referral
            WHERE parent_account_id = $1
            "#,
            account_id
        )
        .fetch_optional(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow::anyhow!("Fail get referral child count Reason :{err}"))?;
        match record {
            Some(record) => Ok(GetReferralChildCountResponse {
                account_id: account_id.to_string(),
                child_count: record.count.unwrap_or(0),
            }),
            None => Ok(GetReferralChildCountResponse {
                account_id: account_id.to_string(),
                child_count: 0,
            }),
        }
    }

    pub async fn get_invited_create(&self, account_id: &str) -> Result<GetInvitedCreateResponse> {
        let record = sqlx::query!(
            r#"
            SELECT account_id,invited_by_create_count
            FROM invited_create
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_optional(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow::anyhow!("Fail get referral child count Reason :{err}"))?;
        match record {
            Some(record) => Ok(GetInvitedCreateResponse {
                account_id: account_id.to_string(),
                invited_by_create_count: record.invited_by_create_count,
            }),
            None => Ok(GetInvitedCreateResponse {
                account_id: account_id.to_string(),
                invited_by_create_count: 0,
            }),
        }
    }

    /// Generates a random alphanumeric code of specified length
    ///
    /// # Arguments
    /// * `length` - The desired length of the generated code
    ///
    /// # Returns
    /// * `String` containing the random code
    ///
    /// # Implementation Details
    /// - Uses thread-safe random number generator
    /// - Generates code using uppercase letters, lowercase letters, and numbers
    fn generate_random_code(length: usize) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();

        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}
