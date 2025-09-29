
/// PostgreSQL 전용 매크로 (기본 1000ms 타임아웃)
#[macro_export]
macro_rules! measure_postgres {
    ($operation:expr, $query:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis(1000), $query).await;

        let elapsed = start_time.elapsed().as_millis() as u64;
        $crate::metrics::METRICS.db.record_postgres_query(elapsed);
        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.db.increment_postgres_timeout();
                tracing::warn!(
                    "Database actual timeout - postgres {} (1000ms timeout)",
                    $operation
                );
                anyhow::anyhow!("Query timeout after 1000ms")
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        match elapsed >= 1000 {
            true => {
                tracing::warn!(
                    "Database slow query - postgres {} ({}ms >= 1000ms threshold)",
                    $operation,
                    elapsed
                );
            }
            false => {
                tracing::info!(
                    "Database query completed - postgres {} ({}ms < 1000ms threshold)",
                    $operation,
                    elapsed
                );
            }
        }

        anyhow::Ok(query_result)
    }};
}

/// Redis 전용 매크로 (기본 300ms 타임아웃)
#[macro_export]
macro_rules! measure_redis {
    ($operation:expr, $query:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis(300), $query).await;

        let elapsed = start_time.elapsed().as_millis() as u64;
        $crate::metrics::METRICS.db.record_redis_query(elapsed);
        // 모든 쿼리의 실행 시간을 기록

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.db.increment_redis_timeout();
                tracing::warn!(
                    "Database actual timeout - redis {} (300ms timeout)",
                    $operation
                );
                anyhow::anyhow!("Query timeout after 300ms")
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        match elapsed >= 300 {
            true => {
                tracing::warn!(
                    "Database slow query - redis {} ({}ms >= 300ms threshold)",
                    $operation,
                    elapsed
                );
            }
            false => {
                tracing::info!(
                    "Database query completed - redis {} ({}ms < 300ms threshold)",
                    $operation,
                    elapsed
                );
            }
        }
        // 성공/실패 상관없이 응답시간 기록

        anyhow::Ok(query_result)
    }};
}
