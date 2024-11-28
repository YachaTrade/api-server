pub mod controller;
pub mod model;
use std::time::Duration;

use crate::env::DBEnv;
#[derive(Debug)]
pub struct PostgresDatabase {
    pub write_pool: sqlx::Pool<sqlx::Postgres>,
    pub read_pool: sqlx::Pool<sqlx::Postgres>,
}

async fn connect_primary() -> sqlx::Pool<sqlx::Postgres> {
    let env = DBEnv::new();
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        env.primary_db_user,
        env.primary_db_password,
        env.primary_db_host,
        env.primary_db_port,
        env.primary_db_name
    );

    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(50)
        .max_lifetime(Duration::from_secs(60 * 60 * 24))
        .connect(url.as_str())
        .await
        .expect("Failed to connect to primary database");
    pool
}

async fn connect_replica() -> sqlx::Pool<sqlx::Postgres> {
    let env = DBEnv::new();
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        env.replica_db_user,
        env.replica_db_password,
        env.replica_db_host,
        env.replica_db_port,
        env.replica_db_name
    );

    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(50)
        .max_lifetime(Duration::from_secs(60 * 60 * 24))
        .connect(url.as_str())
        .await
        .expect("Failed to connect to replica database");
    pool
}

impl PostgresDatabase {
    pub async fn new() -> Self {
        let write_pool = connect_primary().await;
        let read_pool = connect_replica().await;
        PostgresDatabase {
            write_pool,
            read_pool,
        }
    }

    // 읽기 작업을 위한 풀 반환
    pub fn get_read_pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.read_pool
    }

    // 쓰기 작업을 위한 풀 반환
    pub fn get_write_pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.write_pool
    }
}
/*  sqlx::query: 구조체로 매핑할 필요 없이 쿼리를 실행할 때 사용
•	sqlx::query_as!: 쿼리 결과를 구조체로 매핑할 때 사용
•	sqlx::query!: 결과를 튜플로 가져오거나, 단순히 쿼리를 실행할 때 사용
*/
