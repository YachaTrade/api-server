# Database Timeout Configuration

## 개요

API 서버의 데이터베이스 타임아웃을 ECS Task 환경변수로 설정할 수 있습니다.

## 환경변수

### Redis Timeout
```bash
REDIS_TIMEOUT_MS=5000
```
- **설명**: Redis 쿼리 타임아웃 (밀리초)
- **기본값**: 5000ms (5초)
- **권장값**: 3000-10000ms
- **적용 대상**: Redis Serverless의 모든 GET/SET 작업

### PostgreSQL Timeout
```bash
POSTGRES_TIMEOUT_MS=10000
```
- **설명**: PostgreSQL 쿼리 타임아웃 (밀리초)
- **기본값**: 10000ms (10초)
- **권장값**: 5000-30000ms
- **적용 대상**: Primary/Replica의 모든 SELECT/INSERT/UPDATE/DELETE 쿼리

## ECS Task Definition 설정 예시

### JSON 형식
```json
{
  "containerDefinitions": [
    {
      "name": "api-server",
      "environment": [
        {
          "name": "REDIS_TIMEOUT_MS",
          "value": "5000"
        },
        {
          "name": "POSTGRES_TIMEOUT_MS",
          "value": "10000"
        }
      ]
    }
  ]
}
```

### Terraform 예시
```hcl
resource "aws_ecs_task_definition" "api_server" {
  family = "api-server"

  container_definitions = jsonencode([
    {
      name  = "api-server"
      image = "your-ecr-repo/api-server:latest"

      environment = [
        {
          name  = "REDIS_TIMEOUT_MS"
          value = "5000"
        },
        {
          name  = "POSTGRES_TIMEOUT_MS"
          value = "10000"
        }
      ]
    }
  ])
}
```

## 상황별 권장 설정

### 일반 운영 환경
```bash
REDIS_TIMEOUT_MS=5000      # 5초
POSTGRES_TIMEOUT_MS=10000  # 10초
```

### 높은 부하 환경
```bash
REDIS_TIMEOUT_MS=10000     # 10초
POSTGRES_TIMEOUT_MS=20000  # 20초
```

### 개발/테스트 환경
```bash
REDIS_TIMEOUT_MS=3000      # 3초 (빠른 피드백)
POSTGRES_TIMEOUT_MS=5000   # 5초 (빠른 피드백)
```

### 스트레스 테스트 환경
```bash
REDIS_TIMEOUT_MS=15000     # 15초
POSTGRES_TIMEOUT_MS=30000  # 30초
```

## 모니터링

### CloudWatch Logs 확인

**타임아웃 발생 시:**
```
WARN Database actual timeout - redis redis.set_market (5000ms timeout)
WARN Database actual timeout - postgres postgres.get_token (10000ms timeout)
```

**느린 쿼리 경고:**
```
WARN Database slow query - redis redis.get_token (4500ms >= 5000ms threshold)
WARN Database slow query - postgres postgres.get_holders (9500ms >= 10000ms threshold)
```

**정상 쿼리:**
```
INFO Database query completed - redis redis.get_token (150ms < 5000ms threshold)
INFO Database query completed - postgres postgres.get_profile (2500ms < 10000ms threshold)
```

### CloudWatch Metrics

다음 메트릭으로 타임아웃 발생 빈도를 확인할 수 있습니다:
- `db_redis_timeout_count`: Redis 타임아웃 발생 횟수
- `db_postgres_timeout_count`: PostgreSQL 타임아웃 발생 횟수
- `db_redis_query_duration`: Redis 쿼리 응답 시간
- `db_postgres_query_duration`: PostgreSQL 쿼리 응답 시간

## 타임아웃 튜닝 가이드

### 1. 현재 응답 시간 확인
```bash
# CloudWatch Logs Insights 쿼리
fields @timestamp, @message
| filter @message like /Database query completed/
| parse @message /(\d+)ms/
| stats avg($1) as avg_ms, max($1) as max_ms, pct($1, 95) as p95_ms by bin(5m)
```

### 2. 타임아웃 조정
- **p95 < 타임아웃의 50%**: 타임아웃을 낮춰도 됨
- **p95 > 타임아웃의 70%**: 타임아웃을 높여야 함
- **p99 > 타임아웃**: 반드시 타임아웃을 높여야 함

### 3. 예시
```
현재 설정: POSTGRES_TIMEOUT_MS=10000
실제 측정:
  - p50: 500ms
  - p95: 8000ms  (타임아웃의 80% → 높여야 함)
  - p99: 12000ms (타임아웃 초과 → 반드시 높여야 함)

권장: POSTGRES_TIMEOUT_MS=20000
```

## 주의사항

### ⚠️ 타임아웃을 너무 낮게 설정하면
- 정상 쿼리도 타임아웃 처리됨
- 에러율 증가
- 사용자 경험 악화

### ⚠️ 타임아웃을 너무 높게 설정하면
- 실제 문제가 있는 쿼리 발견이 늦어짐
- 리소스 낭비
- 느린 응답 시간 허용

### ✅ 권장 사항
1. **모니터링 먼저**: CloudWatch Logs/Metrics로 현재 상태 파악
2. **점진적 조정**: 한 번에 50% 이상 변경하지 않기
3. **스트레스 테스트**: 변경 후 부하 테스트로 검증
4. **알람 설정**: 타임아웃 발생 시 알림 받기

## 트러블슈팅

### Redis 타임아웃이 자주 발생하는 경우
1. Redis Serverless 상태 확인 (AWS Console)
2. 네트워크 레이턴시 확인
3. 쿼리 복잡도 확인
4. 타임아웃 값 증가 고려

### PostgreSQL 타임아웃이 자주 발생하는 경우
1. 슬로우 쿼리 로그 확인
2. DB 인덱스 확인
3. Connection Pool 설정 확인
4. Read Replica 추가 고려
5. 쿼리 최적화 필요

## 변경 이력

- **2025-11-20**: 초기 버전 작성
  - Redis: 300ms → 5000ms (환경변수 지원)
  - PostgreSQL: 2000ms → 10000ms (환경변수 지원)
