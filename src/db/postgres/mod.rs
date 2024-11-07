pub mod controller;
pub mod model;
use std::time::Duration;

use crate::env::DBEnv;
#[derive(Debug)]
pub struct PostgresDatabase {
    pub pool: sqlx::Pool<sqlx::Postgres>,
}
/*  sqlx::query: 구조체로 매핑할 필요 없이 쿼리를 실행할 때 사용
•	sqlx::query_as!: 쿼리 결과를 구조체로 매핑할 때 사용
•	sqlx::query!: 결과를 튜플로 가져오거나, 단순히 쿼리를 실행할 때 사용
*/
async fn sqlx_connect() -> sqlx::Pool<sqlx::Postgres> {
    let env = DBEnv::new();
    //postgres://user:password@localhost:5432/dbname
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        env.user, env.password, env.host, env.port, env.db_name
    );
    use sqlx::postgres::PgPoolOptions;

    //max_connection = 애플리케이션의 동시 요청 수를 고려하여 적절한 연결 수
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .max_lifetime(Duration::from_secs(60 * 60 * 24))
        .connect(url.as_str())
        .await
        .expect("Failed to connect to database");
    pool
}

impl PostgresDatabase {
    pub async fn new() -> Self {
        let pool = sqlx_connect().await;
        PostgresDatabase { pool }
    }
}
