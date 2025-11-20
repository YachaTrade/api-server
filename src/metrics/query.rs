/// PostgreSQL 전용 매크로 (기본 10000ms 타임아웃, 환경변수로 조정 가능)
#[macro_export]
macro_rules! measure_postgres {
    ($operation:expr, $query:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 환경변수에서 타임아웃 값 읽기 (기본값: 10000ms)
        let timeout_ms = *$crate::config::POSTGRES_TIMEOUT_MS;

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), $query).await;

        let elapsed = start_time.elapsed().as_millis() as u64;
        $crate::metrics::METRICS.db.record_postgres_query(elapsed);
        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.db.increment_postgres_timeout();
                tracing::warn!(
                    "Database actual timeout - postgres {} ({}ms timeout)",
                    $operation,
                    timeout_ms
                );
                anyhow::anyhow!("Query timeout after {}ms", timeout_ms)
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        match elapsed >= timeout_ms {
            true => {
                tracing::warn!(
                    "Database slow query - postgres {} ({}ms >= {}ms threshold)",
                    $operation,
                    elapsed,
                    timeout_ms
                );
            }
            false => {
                tracing::info!(
                    "Database query completed - postgres {} ({}ms < {}ms threshold)",
                    $operation,
                    elapsed,
                    timeout_ms
                );
            }
        }

        anyhow::Ok(query_result)
    }};
}

/// Redis 전용 매크로 (기본 5000ms 타임아웃, 환경변수로 조정 가능)
#[macro_export]
macro_rules! measure_redis {
    ($operation:expr, $query:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 환경변수에서 타임아웃 값 읽기 (기본값: 5000ms)
        let timeout_ms = *$crate::config::REDIS_TIMEOUT_MS;

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), $query).await;

        let elapsed = start_time.elapsed().as_millis() as u64;
        $crate::metrics::METRICS.db.record_redis_query(elapsed);
        // 모든 쿼리의 실행 시간을 기록

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.db.increment_redis_timeout();
                tracing::warn!(
                    "Database actual timeout - redis {} ({}ms timeout)",
                    $operation,
                    timeout_ms
                );
                anyhow::anyhow!("Query timeout after {}ms", timeout_ms)
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        match elapsed >= timeout_ms {
            true => {
                tracing::warn!(
                    "Database slow query - redis {} ({}ms >= {}ms threshold)",
                    $operation,
                    elapsed,
                    timeout_ms
                );
            }
            false => {
                tracing::info!(
                    "Database query completed - redis {} ({}ms < {}ms threshold)",
                    $operation,
                    elapsed,
                    timeout_ms
                );
            }
        }
        // 성공/실패 상관없이 응답시간 기록

        anyhow::Ok(query_result)
    }};
}
