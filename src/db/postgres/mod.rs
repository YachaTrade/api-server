use sqlx::{Executor, Postgres};
use std::{env, str::FromStr, time::Duration};
use tracing::info;

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
        // Neon과 PgBouncer 트랜잭션 풀링 모드에 최적화
        // PgBouncer가 이미 연결 풀링을 처리하므로 최대 연결 수를 낮게 설정
        .max_connections(50)
        // 콜드 스타트 방지를 위해 최소 연결 수 유지
        .min_connections(5)
        // Neon 서버리스 환경에 맞는 짧은 연결 수명
        .max_lifetime(Duration::from_secs(10 * 60))
        // PgBouncer가 빠르게 연결을 제공하므로 짧은 획득 타임아웃 설정
        .acquire_timeout(Duration::from_secs(15))
        // 서버리스 환경에서 리소스 해제를 위한 적극적인 유휴 타임아웃
        .idle_timeout(Duration::from_secs(60))
        // PgBouncer가 연결 테스트를 처리하므로 생략
        .test_before_acquire(false)
        // 연결 초기화 - PgBouncer에 최적화된 최소 설정
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // TCP keepalive 설정 - Neon 서버리스 아키텍처에 중요
                conn.execute("SET tcp_keepalives_idle = '15'").await?;
                conn.execute("SET tcp_keepalives_interval = '5'").await?;
                conn.execute("SET tcp_keepalives_count = '3'").await?;

                // 장시간 실행 쿼리 방지를 위한 문장 타임아웃 설정
                conn.execute("SET statement_timeout = '30s'").await?;
                // PgBouncer에서 트랜잭션 내 유휴 연결 해제를 위해 중요
                conn.execute("SET idle_in_transaction_session_timeout = '15s'")
                    .await?;
                Ok(())
            })
        })
        // 더 상세한 연결 설정을 위해 PgConnectOptions 사용
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&primary_db_url)
                .expect("Invalid primary database URL")
                .application_name("nads-pump-writer")
                // PgBouncer 트랜잭션 풀링 모드를 위한 더 큰 문장 캐시
                .statement_cache_capacity(1000),
        )
        .await
        .expect("Failed to connect to primary database");

    info!("Neon PostgreSQL 프라이머리 풀이 PgBouncer 최적화 설정으로 초기화되었습니다");
    pool
}

async fn connect_replica() -> sqlx::Pool<sqlx::Postgres> {
    use sqlx::postgres::PgPoolOptions;
    let replica_db_url =
        env::var("REPLICA_DATABASE_URL").expect("REPLICA_DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        // Neon과 PgBouncer 트랜잭션 풀링 모드에 최적화
        // 읽기 작업을 위해 더 많은 연결 허용, 여전히 PgBouncer 고려
        .max_connections(1000)
        // 즉시 읽기 작업을 위한 충분한 최소 연결 유지
        .min_connections(10)
        // Neon 서버리스 환경에 맞는 짧은 수명
        .max_lifetime(Duration::from_secs(10 * 60))
        // PgBouncer가 빠르게 응답해야 하므로 짧은 획득 타임아웃
        .acquire_timeout(Duration::from_secs(15))
        // 서버리스 환경에서 리소스 해제를 위한 적극적인 유휴 타임아웃
        .idle_timeout(Duration::from_secs(60))
        // PgBouncer가 연결 상태를 관리하므로 테스트 생략
        .test_before_acquire(false)
        // 읽기 작업에 최적화된 연결 초기화
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // Neon을 위한 TCP keepalive 설정
                conn.execute("SET tcp_keepalives_idle = '15'").await?;
                conn.execute("SET tcp_keepalives_interval = '5'").await?;
                conn.execute("SET tcp_keepalives_count = '3'").await?;

                // 읽기 작업을 위한 더 짧은 문장 타임아웃 설정
                conn.execute("SET statement_timeout = '15s'").await?;
                // PgBouncer에서 트랜잭션 내 유휴 연결 해제를 위해 중요
                conn.execute("SET idle_in_transaction_session_timeout = '15s'")
                    .await?;
                // 복제본 연결에 읽기 전용 모드 강제 적용
                conn.execute("SET default_transaction_read_only = 'on'")
                    .await?;
                Ok(())
            })
        })
        // 더 상세한 연결 설정을 위해 PgConnectOptions 사용
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&replica_db_url)
                .expect("Invalid replica database URL")
                .application_name("nads-pump-reader")
                // PgBouncer 환경에서 읽기 작업을 위한 더 큰 문장 캐시
                .statement_cache_capacity(2000),
        )
        .await
        .expect("Failed to connect to replica database");

    info!("Neon PostgreSQL 복제본 풀이 PgBouncer 최적화 설정으로 초기화되었습니다");
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
