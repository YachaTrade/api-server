use std::{env, time::Duration};

#[derive(Debug)]
pub struct PostgresDatabase {
    pub write_pool: sqlx::Pool<sqlx::Postgres>,
    pub read_pool: sqlx::Pool<sqlx::Postgres>,
}

async fn connect_primary() -> sqlx::Pool<sqlx::Postgres> {
    use sqlx::postgres::PgPoolOptions;
    let primary_db_url =
        env::var("PRIMARY_DATABASE_URL").expect("PRIMARY_DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .max_lifetime(Duration::from_secs(60 * 60 * 24))
        .connect(primary_db_url.as_str())
        .await
        .expect("Failed to connect to primary database");
    pool
}

async fn connect_replica() -> sqlx::Pool<sqlx::Postgres> {
    use sqlx::postgres::PgPoolOptions;
    let replica_db_url =
        env::var("REPLICA_DATABASE_URL").expect("REPLICA_DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .max_lifetime(Duration::from_secs(60 * 60 * 24))
        .connect(replica_db_url.as_str())
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
