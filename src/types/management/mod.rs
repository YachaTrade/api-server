use crate::{
    db::postgres::PostgresDatabase,
    types::common::{
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
        CountRow,
    },
    utils::{
        valid_evm_address,
        single_flight::{with_cache, GLOBAL_CACHE},
    },
    cache_key,
};
use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::Row;
use sqlx::types::Json;
use std::{
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::try_join;
use tracing::info;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DevPosition {
    pub token_info: TokenInfo,
    pub market_cap: String,
    pub treasury_amount: String,
    pub balance: String,
    pub fee_amount: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DevPositionsResponse {
    pub positions: Vec<DevPosition>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenManagement {
    pub token_info: TokenInfo,
    pub market_cap: String,
    pub treasury_amount: String,
    pub balance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldingTokenManagementResponse {
    pub managements: Vec<TokenManagement>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenLock {
    pub token_info: TokenInfo,
    pub market_cap: String,
    pub locked_amount: String,
    pub lock_id: i64,
    pub lock_at: i64,
    pub unlock_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenLockResponse {
    pub token_locks: Vec<TokenLock>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnlockInfo {
    pub lock_id: i64,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WithdrawableLock {
    pub token_info: TokenInfo,
    pub market_cap: String,
    pub withdrawable_amount: String,
    pub unlock_info: Vec<UnlockInfo>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WithdrawableLockResponse {
    pub withdrawable_locks: Vec<WithdrawableLock>,
    pub total_count: i64,
}

/// Combined query parameters for swap history
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct ManagementHistoryQuery {
    // PaginationParams fields
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "validate_limit")]
    pub limit: i64,
    #[serde(default = "default_direction", deserialize_with = "validate_direction")]
    pub direction: String,
    /// Minimum volume filter (native amount)
    #[serde(default)]
    pub min_volume: Option<String>,

    /// Account ID for own trades filter
    #[serde(default)]
    pub account_id: Option<String>,

    /// Trade type filter: "BUY", "SELL", or "ALL" (default)
    #[serde(default = "default_activity_type")]
    pub activity_type: String,
}

impl ManagementHistoryQuery {
    /// Validate the query parameters
    pub fn validate(&self) -> Result<(), String> {
        // Validate min_volume if provided
        if let Some(min_vol) = &self.min_volume {
            if min_vol.parse::<f64>().is_err() {
                return Err("Invalid min_volume: must be a valid number".to_string());
            }
        }

        if self.account_id.is_some() {
            if !valid_evm_address(self.account_id.as_ref().unwrap()) {
                return Err("Invalid account ID format".to_string());
            }
        }

        // Validate activity_type
        if self.activity_type != "ALL" && ActivityType::from_str(&self.activity_type).is_err() {
            return Err(
                "Invalid activity_type: must be 'ALL', 'Lock', 'Withdraw', 'Airdrop', or 'Burn'"
                    .to_string(),
            );
        }

        Ok(())
    }
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    10
}

fn default_direction() -> String {
    "DESC".to_string()
}

fn default_activity_type() -> String {
    "ALL".to_string()
}

fn validate_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if limit < 1 || limit > 100 {
        return Err(serde::de::Error::custom(
            "Invalid limit: must be between 1 and 100",
        ));
    }
    Ok(limit)
}

fn validate_direction<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let direction = String::deserialize(deserializer)?;
    let direction_upper = direction.to_uppercase();
    if !["ASC", "DESC"].contains(&direction_upper.as_str()) {
        return Err(serde::de::Error::custom(
            "Invalid direction: must be 'ASC' or 'DESC'",
        ));
    }
    Ok(direction_upper)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ActivityType {
    Lock,
    Withdraw,
    Airdrop,
    Burn,
}
impl FromStr for ActivityType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "LOCK" => Ok(ActivityType::Lock),
            "WITHDRAW" => Ok(ActivityType::Withdraw),
            "AIRDROP" => Ok(ActivityType::Airdrop),
            "BURN" => Ok(ActivityType::Burn),
            _ => Err(format!("Invalid activity type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ManagementHistory {
    pub activity: String,
    pub account_info: AccountInfo,
    pub token_amount: String,
    pub treasury_amount: String,
    pub transaction_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ManagementHistoryResponse {
    pub histories: Vec<ManagementHistory>,
    pub total_count: i64,
}
pub struct TokenManagementController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenManagementController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenManagementController { db }
    }

    pub async fn get_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DevPositionsResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!(
            "dev_positions",
            account_id,
            pagination.page,
            pagination.limit,
            pagination.direction
        );
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination;
            async move {
                let controller = TokenManagementController::new(db);
                controller.fetch_dev_positions(&account_id, &pagination).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_dev_positions completed in {:?} for account_id: {}, page: {}, limit: {}",
            elapsed, account_id, pagination.page, pagination.limit
        );
        Ok(response)
    }
    
    async fn fetch_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DevPositionsResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        pub struct DevPositionRow {
            pub token_id: String,
            pub name: String,
            pub symbol: String,
            pub image_uri: String,
            pub market_cap: BigDecimal,
            pub balance: BigDecimal,
            pub treasury_amount: BigDecimal,
            pub creator_vault_balance: BigDecimal,
        }

        let query = format!(
            r#"
            SELECT 
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri,
                (t.total_supply * COALESCE(m.price, 0)) AS market_cap,
                COALESCE(b.balance, 0) AS balance,
                COALESCE(ttb.total_lock, 0) AS treasury_amount,
                COALESCE(cvb.native_amount, 0) AS creator_vault_balance
            FROM 
                token t
            JOIN 
                market m ON t.token_id = m.token_id
            LEFT JOIN 
                token_management_total_lock ttb ON t.token_id = ttb.token_id
            LEFT JOIN 
                creator_vault_balance cvb ON t.token_id = cvb.token_id
            LEFT JOIN 
                balance b ON t.creator = b.account_id
            WHERE 
                t.creator = $1
            ORDER BY 
                cvb.native_amount {}
            LIMIT $2 OFFSET $3
            "#,
            pagination.direction
        );

        // 첫 번째 비동기 작업: 토큰 데이터 가져오기
        let tokens_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, DevPositionRow>(&query)
                .bind(account_id)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool()),
        );

        // 두 번째 비동기 작업: 총 카운트 가져오기
        let count_query = r#"
            SELECT 
                COUNT(*) as count
            FROM 
                token t
            WHERE 
                t.creator = $1 AND
                t.is_listing = true
        "#;

        let count_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(count_query)
                .bind(account_id)
                .fetch_one(self.db.get_read_pool()),
        );

        // 두 작업을 병렬로 실행
        let (token_result, count_result) = try_join!(tokens_future, count_future)?;
        let token_result = token_result.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let count_result = count_result.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let total_count = count_result.get::<i64, _>("count");

        let positions = token_result
            .into_iter()
            .map(|row| {
                let market_cap = row.market_cap.to_string();
                let treasury_amount: String = row.treasury_amount.to_string();
                let fee_amount = row.creator_vault_balance.to_string();
                let balance = row.balance.to_string();
                DevPosition {
                    token_info: TokenInfo {
                        token_id: row.token_id,
                        name: row.name,
                        symbol: row.symbol,
                        image_uri: row.image_uri,
                    },
                    market_cap: market_cap,
                    treasury_amount,
                    fee_amount,
                    balance,
                }
            })
            .collect();

        Ok(DevPositionsResponse {
            positions,
            total_count,
        })
    }

    pub async fn get_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldingTokenManagementResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!(
            "holding_token_management",
            account_id,
            pagination.page,
            pagination.limit,
            pagination.direction
        );
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination;
            async move {
                let controller = TokenManagementController::new(db);
                controller.fetch_holding_token_management(&account_id, &pagination).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_holding_token_management completed in {:?} for account_id: {}, page: {}, limit: {}",
            elapsed, account_id, pagination.page, pagination.limit
        );
        Ok(response)
    }
    
    async fn fetch_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldingTokenManagementResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct HoldingTokenManagementRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            market_cap: BigDecimal,
            balance: BigDecimal,
            treasury_amount: BigDecimal,
        }

        let query = format!(
            r#"
            SELECT 
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri,
                (t.total_supply * COALESCE(m.price, 0)) AS market_cap,
                b.balance AS balance,
                COALESCE(ttl.total_lock, 0) AS treasury_amount
            FROM 
                balance b
            JOIN 
                token t ON b.token_id = t.token_id
            JOIN 
                market m ON t.token_id = m.token_id
           LEFT JOIN token_management_total_lock ttl ON b.token_id = ttl.token_id
            WHERE 
                b.account_id = $1
                AND b.balance > 0
            ORDER BY 
                b.balance {}
            LIMIT $2 OFFSET $3
            "#,
            pagination.direction
        );

        let rows = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, HoldingTokenManagementRow>(&query)
                .bind(account_id)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        let managements = rows
            .into_iter()
            .map(|row| TokenManagement {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                balance: row.balance.to_string(),
                market_cap: row.market_cap.to_string(),
                treasury_amount: row.treasury_amount.to_string(),
            })
            .collect();

        let total_count = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, CountRow>(
                r#"
                    SELECT 
                        COUNT(*) as count
                    FROM 
                        balance 
                    WHERE 
                        account_id = $1
                        AND balance > 0
                    "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        let total_count = total_count.count;

        Ok(HoldingTokenManagementResponse {
            managements,
            total_count,
        })
    }

    pub async fn get_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenLockResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!(
            "account_locks",
            account_id,
            pagination.page,
            pagination.limit,
            pagination.direction
        );
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination;
            async move {
                let controller = TokenManagementController::new(db);
                controller.fetch_account_locks(&account_id, &pagination).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_account_locks completed in {:?} for account_id: {}, page: {}, limit: {}",
            elapsed, account_id, pagination.page, pagination.limit
        );
        Ok(response)
    }
    
    async fn fetch_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenLockResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let query = format!(
            r#"
            SELECT 
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri,
                (t.total_supply * COALESCE(m.price, 0)) AS market_cap,
                COALESCE(tml.token_amount, 0) AS locked_amount,
                tml.lock_id,
                tml.lock_at,
                tml.unlock_at
            FROM 
                token_management_lock tml
            JOIN 
                token t ON tml.token_id = t.token_id
            JOIN 
                market m ON t.token_id = m.token_id
            WHERE 
                tml.account_id = $1 AND unlock_at > $2
            ORDER BY 
                tml.lock_at {}
            LIMIT $3 OFFSET $4
            "#,
            pagination.direction
        );

        /// Represents token lock information with related token details
        #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
        pub struct TokenLockInfoRow {
            pub token_id: String,
            pub name: String,
            pub symbol: String,
            pub image_uri: String,
            pub market_cap: BigDecimal,
            pub locked_amount: BigDecimal,
            pub lock_id: i64,
            pub lock_at: i64,
            pub unlock_at: i64,
        }

        let now = chrono::Utc::now().timestamp();

        let token_lock_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, TokenLockInfoRow>(&query)
                .bind(account_id)
                .bind(now)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool()),
        );

        let count_query = r#"
            SELECT 
                COUNT(*) as count
            FROM 
                token_management_lock
            WHERE 
                account_id = $1
                AND unlock_at > $2
            "#;

        let count_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(count_query)
                .bind(account_id)
                .bind(now)
                .fetch_one(self.db.get_read_pool()),
        );

        let (count_result, token_locks) = try_join!(count_future, token_lock_future)?;
        let count_result = count_result.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let token_locks = token_locks.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let total_count = count_result.get::<i64, _>("count");

        let token_locks = token_locks
            .into_iter()
            .map(|row| TokenLock {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                market_cap: row.market_cap.to_string(),
                locked_amount: row.locked_amount.to_string(),
                lock_id: row.lock_id,
                lock_at: row.lock_at,
                unlock_at: row.unlock_at,
            })
            .collect();

        Ok(TokenLockResponse {
            token_locks,
            total_count,
        })
    }

    pub async fn get_account_withdrawable_lock(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<WithdrawableLockResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!(
            "account_withdrawable_lock",
            account_id,
            pagination.page,
            pagination.limit,
            pagination.direction
        );
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let account_id = account_id.to_string();
            let pagination = pagination;
            async move {
                let controller = TokenManagementController::new(db);
                controller.fetch_account_withdrawable_lock(&account_id, &pagination).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_account_withdrawable_lock completed in {:?} for account_id: {}, page: {}, limit: {}",
            elapsed, account_id, pagination.page, pagination.limit
        );
        Ok(response)
    }
    
    async fn fetch_account_withdrawable_lock(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<WithdrawableLockResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let withdrawable_lock_query = format!(
            r#"
            SELECT 
                t.token_id,
                t.name,
                t.symbol,
                t.image_uri,
                (t.total_supply * COALESCE(m.price, 0)) AS market_cap,
                SUM(COALESCE(tml.token_amount, 0)) AS withdrawable_amount,
                json_agg(
                    json_build_object(
                        'lock_id', tml.lock_id,
                        'amount', tml.token_amount
                    )
                ) AS unlock_info
            FROM 
                token_management_lock tml
            JOIN 
                token t ON tml.token_id = t.token_id
            JOIN 
                market m ON t.token_id = m.token_id
            WHERE 
                tml.account_id = $1 AND unlock_at <= $2
            GROUP BY 
                t.token_id, t.name, t.symbol, t.image_uri, t.total_supply, m.price
            ORDER BY 
                SUM(tml.token_amount * COALESCE(m.price, 0)) {}
            LIMIT $3 OFFSET $4
            "#,
            pagination.direction
        );

        // 여기서 구조체 정의를 쿼리 결과와 일치하도록 수정
        #[derive(Debug, sqlx::FromRow)]
        struct WithdrawableLockRow {
            token_id: String,
            name: String,
            symbol: String,
            image_uri: String,
            market_cap: BigDecimal,
            withdrawable_amount: BigDecimal,
            unlock_info: Json<Vec<serde_json::Value>>, // JSON 타입으로 수정
        }

        let now = chrono::Utc::now().timestamp();

        let withdrawable_lock_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query_as::<_, WithdrawableLockRow>(&withdrawable_lock_query)
                .bind(account_id)
                .bind(now)
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(self.db.get_read_pool()),
        );

        let withdrawable_lock_count_query = r#"
            SELECT 
                COUNT(DISTINCT token_id) as count
            FROM 
                token_management_lock
            WHERE 
                account_id = $1
                AND unlock_at <= $2
            "#;

        let count_future = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(withdrawable_lock_count_query)
                .bind(account_id)
                .bind(now)
                .fetch_one(self.db.get_read_pool()),
        );

        let (count_result, withdrawable_locks_raw) =
            try_join!(count_future, withdrawable_lock_future)?;
        let count_result = count_result.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let withdrawable_locks_raw =
            withdrawable_locks_raw.map_err(|_| anyhow!("Query timeout after 500ms"))?;
        let total_count = count_result.get::<i64, _>("count");

        // JSON 데이터를 파싱하고 구조체로 변환
        let withdrawable_locks = withdrawable_locks_raw
            .into_iter()
            .map(|row| {
                // sqlx::types::Json의 내부 값에 접근
                let unlock_info: Vec<UnlockInfo> = row
                    .unlock_info
                    .0
                    .into_iter()
                    .map(|json_value| UnlockInfo {
                        lock_id: json_value["lock_id"].as_i64().unwrap_or_default(),
                        amount: json_value["amount"].to_string().replace('\"', ""),
                    })
                    .collect();

                WithdrawableLock {
                    token_info: TokenInfo {
                        token_id: row.token_id,
                        name: row.name,
                        symbol: row.symbol,
                        image_uri: row.image_uri,
                    },
                    market_cap: row.market_cap.to_string(),
                    withdrawable_amount: row.withdrawable_amount.to_string(),
                    unlock_info,
                }
            })
            .collect();

        Ok(WithdrawableLockResponse {
            withdrawable_locks,
            total_count,
        })
    }

    pub async fn get_management_history(
        &self,
        token_id: &str,
        query: &ManagementHistoryQuery,
    ) -> Result<ManagementHistoryResponse> {
        let start_time = Instant::now();
        
        // 캐시 키 생성
        let cache_key = cache_key!(
            "management_history",
            token_id,
            query.page,
            query.limit,
            query.direction,
            query.activity_type,
            query.min_volume.as_ref().unwrap_or(&"".to_string()),
            query.account_id.as_ref().unwrap_or(&"".to_string())
        );
        
        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let token_id = token_id.to_string();
            let query = query.clone();
            async move {
                let controller = TokenManagementController::new(db);
                controller.fetch_management_history(&token_id, &query).await
            }
        })
        .await?;
        
        let elapsed = start_time.elapsed();
        info!(
            "get_management_history completed in {:?} for token_id: {}, page: {}, limit: {}",
            elapsed, token_id, query.page, query.limit
        );
        Ok(response)
    }
    
    async fn fetch_management_history(
        &self,
        token_id: &str,
        query: &ManagementHistoryQuery,
    ) -> Result<ManagementHistoryResponse> {
        let offset = (query.page - 1) * query.limit;

        // 파라미터 카운터로 순서 관리
        let mut param_count = 1;
        let mut query_sql = r#"
        SELECT 
            th.token_amount,
            th.history_type AS activity,
            th.created_at,
            th.transaction_hash,
            COALESCE(ttl.total_lock, 0) AS treasury_amount,
            a.account_id,
            a.nickname as account_nickname,
            a.image_uri as account_image,
            a.follower_count,
            a.following_count,
            ax.x_handle,
            ax.x_image_uri,
            ax.is_blue_label
        FROM token_management_history th
        JOIN account a ON th.account_id = a.account_id
        LEFT JOIN account_x ax ON a.account_id = ax.account_id
        LEFT JOIN token_management_total_lock ttl ON th.token_id = ttl.token_id
        WHERE th.token_id = $1"#
            .to_string();

        param_count += 1; // token_id는 $1

        // Add own trades filter
        if let Some(_) = &query.account_id {
            query_sql.push_str(&format!(" AND th.account_id = ${}", param_count));
            param_count += 1;
        }

        // Add trade type filter
        // Add activity type filter
        match query.activity_type.as_str() {
            "LOCK" => query_sql.push_str(" AND th.history_type = 'LOCK'"),
            "WITHDRAW" => query_sql.push_str(" AND th.history_type = 'WITHDRAW'"),
            "AIRDROP" => query_sql.push_str(" AND th.history_type = 'AIRDROP'"),
            "BURN" => query_sql.push_str(" AND th.history_type = 'BURN'"),
            _ => {} // "ALL" - no filter
        }
        // Add volume filters
        if let Some(_) = &query.min_volume {
            query_sql.push_str(&format!(" AND th.token_amount >= ${}", param_count));
            param_count += 1;
        }
        // 정렬 방향 추가
        query_sql.push_str(&format!(" ORDER BY th.created_at {}", query.direction));
        query_sql.push_str(&format!(" LIMIT {} OFFSET {}", query.limit, offset));

        // 쿼리 준비 및 파라미터 바인딩 (순서대로)
        let mut query_builder = sqlx::query(&query_sql);

        // 첫 번째 파라미터 바인딩 (token_id)
        query_builder = query_builder.bind(token_id);

        // 조건부 파라미터 바인딩 (순서 보장)
        if let Some(min_vol) = &query.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        if let Some(account_id) = &query.account_id {
            query_builder = query_builder.bind(account_id);
        }

        // 쿼리 실행
        let rows = tokio::time::timeout(
            Duration::from_millis(500),
            query_builder.fetch_all(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        // 결과 변환 (기존과 동일)
        let histories: Vec<ManagementHistory> = rows
            .into_iter()
            .map(|row| {
                let account_id: String = row.try_get("account_id").unwrap();
                let account_nickname: String = row.try_get("account_nickname").unwrap();
                let account_image: String = row.try_get("account_image").unwrap();
                let follower_count: i32 = row.try_get("follower_count").unwrap();
                let following_count: i32 = row.try_get("following_count").unwrap();
                let token_amount: BigDecimal = row.try_get("token_amount").unwrap();
                let treasury_amount: BigDecimal = row.try_get("treasury_amount").unwrap();
                let activity: String = row.try_get("activity").unwrap();
                let created_at: i64 = row.try_get("created_at").unwrap();
                let transaction_hash: String = row.try_get("transaction_hash").unwrap();
                let x_handle: Option<String> = row.try_get("x_handle").unwrap();
                let x_image_uri: Option<String> = row.try_get("x_image_uri").unwrap();
                let _is_blue_label: Option<bool> = row.try_get("is_blue_label").unwrap();

                ManagementHistory {
                    account_info: AccountInfo {
                        account_id,
                        nickname: if x_handle.is_some() {
                            x_handle.unwrap()
                        } else {
                            account_nickname
                        },
                        image_uri: if x_image_uri.is_some() {
                            x_image_uri.unwrap()
                        } else {
                            account_image
                        },
                        follower_count,
                        following_count,
                    },
                    activity,
                    token_amount: token_amount.to_string(),
                    treasury_amount: treasury_amount.to_string(),
                    created_at,
                    transaction_hash,
                }
            })
            .collect();

        let total_count = if histories.is_empty() {
            0
        } else {
            self.get_total_count_by_token_with_filters(token_id, &query)
                .await?
        };

        Ok(ManagementHistoryResponse {
            histories,
            total_count,
        })
    }

    // get_total_count_by_token_with_filters 메서드 수정
    async fn get_total_count_by_token_with_filters(
        &self,
        token_id: &str,
        query_params: &ManagementHistoryQuery,
    ) -> Result<i64> {
        // 필터가 없으면 캐시된 count 사용
        // 완전히 필터가 없는 경우 - 전체 count 계산
        if query_params.min_volume.is_none()
            && query_params.account_id.is_none()
            && query_params.activity_type == "ALL"
        {
            // 모든 활동 타입 카운트 합산
            return self.get_cached_total_count(token_id).await;
        }

        // 활동 타입만 있는 경우 - 해당 타입의 카운트 사용
        if query_params.min_volume.is_none() && query_params.account_id.is_none() {
            let column = match query_params.activity_type.as_str() {
                "Lock" | "LOCK" => "lock_count",
                "Withdraw" | "WITHDRAW" => "withdraw_count",
                "Airdrop" | "AIRDROP" => "airdrop_count",
                "Burn" | "BURN" => "burn_count",
                _ => return self.get_cached_total_count(token_id).await, // "ALL"
            };
            return self.get_cached_count(token_id, column).await;
        }

        // 다른 필터가 있는 경우 직접 카운트 쿼리 실행
        let mut param_count = 1;
        let mut query = r#"
            SELECT COALESCE(COUNT(*)::bigint, 0) as count
            FROM token_management_history th
            JOIN account a ON th.account_id = a.account_id
            WHERE th.token_id = $1"#
            .to_string();

        param_count += 1; // token_id는 $1

        // Add volume filters
        if let Some(_) = &query_params.min_volume {
            query.push_str(&format!(" AND th.token_amount >= ${}", param_count));
            param_count += 1;
        }

        // Add account filter
        if let Some(_) = &query_params.account_id {
            query.push_str(&format!(" AND th.account_id = ${}", param_count));
            param_count += 1;
        }

        // Add activity type filter
        match query_params.activity_type.as_str() {
            "ALL" => {} // 전체 조회, 필터 없음
            activity_type => {
                query.push_str(&format!(
                    " AND th.history_type = '{}'",
                    activity_type.to_uppercase()
                ));
            }
        }

        // Execute query using raw SQL
        let mut query_builder = sqlx::query(&query);

        // 첫 번째 파라미터 바인딩 (token_id)
        query_builder = query_builder.bind(token_id);

        // 조건부 파라미터 바인딩 (순서 보장)
        if let Some(min_vol) = &query_params.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        if let Some(account_id) = &query_params.account_id {
            query_builder = query_builder.bind(account_id);
        }

        let row = tokio::time::timeout(
            Duration::from_millis(500),
            query_builder.fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;
        let count: i64 = row.try_get("count").unwrap();
        Ok(count)
    }

    // 캐시된 특정 활동 타입 카운트 조회
    async fn get_cached_count(&self, token_id: &str, column: &str) -> Result<i64> {
        let query = format!(
            "SELECT {} as count FROM token_management_history_count WHERE token_id = $1",
            column
        );

        let row = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(&query)
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(row.map(|r| r.get::<i64, _>("count")).unwrap_or(0))
    }

    // 전체 카운트 조회 (모든 활동 타입의 합)
    // 전체 카운트 조회 (total_count 필드 사용)
    async fn get_cached_total_count(&self, token_id: &str) -> Result<i64> {
        let query = r#"
        SELECT 
            total_count as count 
        FROM token_management_history_count 
        WHERE token_id = $1
        "#;

        let row = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(query)
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(row.map(|r| r.get::<i64, _>("count")).unwrap_or(0))
    }
}
