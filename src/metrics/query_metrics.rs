use crate::metrics::METRICS;
use tracing::warn;

/// PostgreSQL 전용 매크로 (기본 1000ms 타임아웃)
#[macro_export]
macro_rules! measure_postgres {
    ($operation:expr, $query:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(1000), async { $query }).await;

        let elapsed = start_time.elapsed();

        // 모든 쿼리의 실행 시간을 기록
        $crate::metrics::METRICS.record_db_duration("postgres", $operation, elapsed);

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.record_db_timeout("postgres", $operation);
                tracing::warn!(
                    "Database actual timeout - postgres {} (1000ms timeout)",
                    $operation
                );
                anyhow::anyhow!("Query timeout after 1000ms")
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        if elapsed.as_millis() >= 1000 {
            $crate::metrics::METRICS.record_db_timeout("postgres", $operation);
            tracing::warn!(
                "Database slow query - postgres {} ({}ms >= 1000ms threshold)",
                $operation,
                elapsed.as_millis()
            );
        } else {
            tracing::debug!(
                "Database query completed - postgres {} ({}ms)",
                $operation,
                elapsed.as_millis()
            );
        }
        
        anyhow::Ok(query_result)
    }};
    ($operation:expr, $query:expr, $timeout_ms:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis($timeout_ms), async {
            $query
        })
        .await;

        let elapsed = start_time.elapsed();

        // 모든 쿼리의 실행 시간을 기록
        $crate::metrics::METRICS.record_db_duration("postgres", $operation, elapsed);

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.record_db_timeout("postgres", $operation);
                tracing::warn!(
                    "Database actual timeout - postgres {} ({}ms timeout)",
                    $operation,
                    $timeout_ms
                );
                anyhow::anyhow!("Query timeout after {}ms", $timeout_ms)
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        if elapsed.as_millis() >= $timeout_ms {
            $crate::metrics::METRICS.record_db_timeout("postgres", $operation);
            tracing::warn!(
                "Database slow query - postgres {} ({}ms >= {}ms threshold)",
                $operation,
                elapsed.as_millis(),
                $timeout_ms
            );
        } else {
            tracing::debug!(
                "Database query completed - postgres {} ({}ms)",
                $operation,
                elapsed.as_millis()
            );
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
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(300), async { $query }).await;

        let elapsed = start_time.elapsed();

        // 모든 쿼리의 실행 시간을 기록
        $crate::metrics::METRICS.record_db_duration("redis", $operation, elapsed);

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.record_db_timeout("redis", $operation);
                tracing::warn!(
                    "Database actual timeout - redis {} (300ms timeout)",
                    $operation
                );
                anyhow::anyhow!("Query timeout after 300ms")
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        if elapsed.as_millis() >= 300 {
            $crate::metrics::METRICS.record_db_timeout("redis", $operation);
            tracing::warn!(
                "Database slow query - redis {} ({}ms >= 300ms threshold)",
                $operation,
                elapsed.as_millis()
            );
        } else {
            tracing::debug!(
                "Database query completed - redis {} ({}ms)",
                $operation,
                elapsed.as_millis()
            );
        }
        
        anyhow::Ok(query_result)
    }};
    ($operation:expr, $query:expr, $timeout_ms:expr) => {{
        let start_time = tokio::time::Instant::now();

        // 실제 timeout과 메트릭 수집을 함께 적용
        let result = tokio::time::timeout(std::time::Duration::from_millis($timeout_ms), async {
            $query
        })
        .await;

        let elapsed = start_time.elapsed();

        // 모든 쿼리의 실행 시간을 기록
        $crate::metrics::METRICS.record_db_duration("redis", $operation, elapsed);

        // tokio::timeout 결과를 anyhow로 매핑
        let query_result = result
            .map_err(|_| {
                // 실제 타임아웃 발생
                $crate::metrics::METRICS.record_db_timeout("redis", $operation);
                tracing::warn!(
                    "Database actual timeout - redis {} ({}ms timeout)",
                    $operation,
                    $timeout_ms
                );
                anyhow::anyhow!("Query timeout after {}ms", $timeout_ms)
            })?
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        // 쿼리는 완료되었지만 임계값과 비교
        if elapsed.as_millis() >= $timeout_ms {
            $crate::metrics::METRICS.record_db_timeout("redis", $operation);
            tracing::warn!(
                "Database slow query - redis {} ({}ms >= {}ms threshold)",
                $operation,
                elapsed.as_millis(),
                $timeout_ms
            );
        } else {
            tracing::debug!(
                "Database query completed - redis {} ({}ms)",
                $operation,
                elapsed.as_millis()
            );
        }
        
        anyhow::Ok(query_result)
    }};
}