use std::{env, error::Error, str::FromStr, time::Duration};
use tracing::info;

#[derive(Debug)]
pub struct PostgresDatabase {
    pub write_pool: sqlx::Pool<sqlx::Postgres>,
    pub read_pool: sqlx::Pool<sqlx::Postgres>,
}

async fn connect_primary() -> sqlx::Pool<sqlx::Postgres> {
    use sqlx::postgres::PgPoolOptions;
    let primary_db_url = env::var("PRIMARY_DATABASE_URL")
        .unwrap_or_else(|_| panic!("PRIMARY_DATABASE_URL must be set in environment variables"));

    // 환경변수에서 연결 풀 설정 읽기
    let max_connections = env::var("PG_PRIMARY_MAX_CONNECTIONS")
        .expect("PG_PRIMARY_MAX_CONNECTIONS must be set")
        .parse::<u32>()
        .expect("PG_PRIMARY_MAX_CONNECTIONS must be a valid u32");
    let min_connections = env::var("PG_PRIMARY_MIN_CONNECTIONS")
        .expect("PG_PRIMARY_MIN_CONNECTIONS must be set")
        .parse::<u32>()
        .expect("PG_PRIMARY_MIN_CONNECTIONS must be a valid u32");
    let max_lifetime_secs = env::var("PG_PRIMARY_MAX_LIFETIME_SECS")
        .expect("PG_PRIMARY_MAX_LIFETIME_SECS must be set")
        .parse::<u64>()
        .expect("PG_PRIMARY_MAX_LIFETIME_SECS must be a valid u64");
    let acquire_timeout_secs = env::var("PG_PRIMARY_ACQUIRE_TIMEOUT_SECS")
        .expect("PG_PRIMARY_ACQUIRE_TIMEOUT_SECS must be set")
        .parse::<u64>()
        .expect("PG_PRIMARY_ACQUIRE_TIMEOUT_SECS must be a valid u64");
    let idle_timeout_secs = env::var("PG_PRIMARY_IDLE_TIMEOUT_SECS")
        .expect("PG_PRIMARY_IDLE_TIMEOUT_SECS must be set")
        .parse::<u64>()
        .expect("PG_PRIMARY_IDLE_TIMEOUT_SECS must be a valid u64");
    let statement_cache_capacity = env::var("PG_PRIMARY_STATEMENT_CACHE_CAPACITY")
        .expect("PG_PRIMARY_STATEMENT_CACHE_CAPACITY must be set")
        .parse::<usize>()
        .expect("PG_PRIMARY_STATEMENT_CACHE_CAPACITY must be a valid usize");

    let pool = PgPoolOptions::new()
        // Neon과 PgBouncer 트랜잭션 풀링 모드에 최적화
        // PgBouncer가 이미 연결 풀링을 처리하므로 최대 연결 수를 낮게 설정
        .max_connections(max_connections)
        // 콜드 스타트 방지를 위해 최소 연결 수 유지
        .min_connections(min_connections)
        // Neon 서버리스 환경에 맞는 짧은 연결 수명
        .max_lifetime(Duration::from_secs(max_lifetime_secs))
        // PgBouncer가 빠르게 연결을 제공하므로 짧은 획득 타임아웃 설정
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
        // 서버리스 환경에서 리소스 해제를 위한 적극적인 유휴 타임아웃
        .idle_timeout(Duration::from_secs(idle_timeout_secs))
        // PgBouncer가 연결 테스트를 처리하므로 생략
        // .test_before_acquire(true)
        // 연결 초기화 - RDS Proxy 환경에서는 after_connect 설정 제거
        // .after_connect(|conn, _meta| {
        //     Box::pin(async move {
        //         // TCP keepalive 설정 제거 - RDS Proxy가 연결 관리를 담당
        //
        //         // 장시간 실행 쿼리 방지를 위한 문장 타임아웃 설정
        //         conn.execute("SET statement_timeout = '30s'").await?;
        //         // PgBouncer에서 트랜잭션 내 유휴 연결 해제를 위해 중요
        //         conn.execute("SET idle_in_transaction_session_timeout = '15s'")
        //             .await?;
        //         Ok(())
        //     })
        // })
        // 더 상세한 연결 설정을 위해 PgConnectOptions 사용
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&primary_db_url)
                .unwrap_or_else(|_| panic!("Invalid PRIMARY_DATABASE_URL format"))
                .application_name("nads-pump-writer")
                // PgBouncer 트랜잭션 풀링 모드를 위한 더 큰 문장 캐시
                .statement_cache_capacity(statement_cache_capacity),
        )
        .await
        .unwrap_or_else(|err| {
            panic!(
                "Failed to connect to primary PostgreSQL database: {:?}",
                err.source()
            )
        });

    info!("Successfully connected to primary PostgreSQL database");
    pool
}

async fn connect_replica() -> sqlx::Pool<sqlx::Postgres> {
    use sqlx::postgres::PgPoolOptions;
    let replica_db_url = env::var("REPLICA_DATABASE_URL")
        .unwrap_or_else(|_| panic!("REPLICA_DATABASE_URL must be set in environment variables"));

    // 환경변수에서 연결 풀 설정 읽기
    let max_connections = env::var("PG_REPLICA_MAX_CONNECTIONS")
        .expect("PG_REPLICA_MAX_CONNECTIONS must be set")
        .parse::<u32>()
        .expect("PG_REPLICA_MAX_CONNECTIONS must be a valid u32");
    let min_connections = env::var("PG_REPLICA_MIN_CONNECTIONS")
        .expect("PG_REPLICA_MIN_CONNECTIONS must be set")
        .parse::<u32>()
        .expect("PG_REPLICA_MIN_CONNECTIONS must be a valid u32");
    let max_lifetime_secs = env::var("PG_REPLICA_MAX_LIFETIME_SECS")
        .expect("PG_REPLICA_MAX_LIFETIME_SECS must be set")
        .parse::<u64>()
        .expect("PG_REPLICA_MAX_LIFETIME_SECS must be a valid u64");
    let acquire_timeout_secs = env::var("PG_REPLICA_ACQUIRE_TIMEOUT_SECS")
        .expect("PG_REPLICA_ACQUIRE_TIMEOUT_SECS must be set")
        .parse::<u64>()
        .expect("PG_REPLICA_ACQUIRE_TIMEOUT_SECS must be a valid u64");
    let idle_timeout_secs = env::var("PG_REPLICA_IDLE_TIMEOUT_SECS")
        .expect("PG_REPLICA_IDLE_TIMEOUT_SECS must be set")
        .parse::<u64>()
        .expect("PG_REPLICA_IDLE_TIMEOUT_SECS must be a valid u64");
    let statement_cache_capacity = env::var("PG_REPLICA_STATEMENT_CACHE_CAPACITY")
        .expect("PG_REPLICA_STATEMENT_CACHE_CAPACITY must be set")
        .parse::<usize>()
        .expect("PG_REPLICA_STATEMENT_CACHE_CAPACITY must be a valid usize");

    let pool = PgPoolOptions::new()
        // Neon과 PgBouncer 트랜잭션 풀링 모드에 최적화
        // 읽기 작업을 위해 더 많은 연결 허용, 여전히 PgBouncer 고려
        .max_connections(max_connections)
        // 즉시 읽기 작업을 위한 충분한 최소 연결 유지
        .min_connections(min_connections)
        // Neon 서버리스 환경에 맞는 짧은 수명
        .max_lifetime(Duration::from_secs(max_lifetime_secs))
        // PgBouncer가 빠르게 응답해야 하므로 짧은 획득 타임아웃
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
        // 서버리스 환경에서 리소스 해제를 위한 적극적인 유휴 타임아웃
        .idle_timeout(Duration::from_secs(idle_timeout_secs))
        // PgBouncer가 연결 상태를 관리하므로 테스트 생략
        // .test_before_acquire(true)
        // 읽기 작업에 최적화된 연결 초기화 - RDS Proxy 환경에서는 after_connect 설정 제거
        // .after_connect(|conn, _meta| {
        //     Box::pin(async move {
        //         // TCP keepalive 설정 제거 - RDS Proxy가 연결 관리를 담당
        //
        //         // 읽기 작업을 위한 더 짧은 문장 타임아웃 설정
        //         conn.execute("SET statement_timeout = '15s'").await?;
        //         // PgBouncer에서 트랜잭션 내 유휴 연결 해제를 위해 중요
        //         conn.execute("SET idle_in_transaction_session_timeout = '15s'")
        //             .await?;
        //         // 복제본 연결에 읽기 전용 모드 강제 적용
        //         conn.execute("SET default_transaction_read_only = 'on'")
        //             .await?;
        //         Ok(())
        //     })
        // })
        // 더 상세한 연결 설정을 위해 PgConnectOptions 사용
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&replica_db_url)
                .unwrap_or_else(|_| panic!("Invalid REPLICA_DATABASE_URL format"))
                .application_name("nads-pump-reader")
                // PgBouncer 환경에서 읽기 작업을 위한 더 큰 문장 캐시
                .statement_cache_capacity(statement_cache_capacity),
        )
        .await
        .unwrap_or_else(|err| {
            panic!(
                "Failed to connect to replica PostgreSQL database: {:?}  ",
                err.source()
            )
        });

    info!("Successfully connected to replica PostgreSQL database");
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
