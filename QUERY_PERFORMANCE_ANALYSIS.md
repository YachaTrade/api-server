# Query Performance Analysis for 100M Records

이 문서는 각 테이블에 1억 건의 데이터가 있을 때 쿼리 성능을 분석하고 최적화 상태를 평가합니다.

## 평가 기준

- 🟢 **최적화됨**: < 100ms (인덱스 활용, 효율적 실행)
- 🟡 **개선 필요**: 100-500ms (부분 최적화 필요)
- 🔴 **위험**: > 500ms (심각한 최적화 필요)

## 1. Account Controller

### 1.1 Insert/Update Account
```sql
INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count)
VALUES ($1, $2, $3, $4, $5, $6)
ON CONFLICT (account_id) 
DO UPDATE SET...
```
- **예상 실행 시간**: 🟢 < 10ms
- **평가**: 최적화됨
- **이유**: Primary key (account_id)를 사용한 단일 행 작업

### 1.2 Get Account with Mutual Friends
```sql
WITH mutual_friends AS (
    SELECT ... FROM follow f1
    JOIN follow f2 ON f1.following_id = f2.following_id
    JOIN account a ON f1.following_id = a.account_id
    WHERE f1.follower_id = $2 AND f2.follower_id = $1
    ORDER BY a.follower_count DESC
    LIMIT 3
)
```
- **예상 실행 시간**: 🟡 200-300ms
- **평가**: 개선 필요
- **필요한 인덱스**:
  ```sql
  CREATE INDEX idx_follow_follower_following ON follow(follower_id, following_id);
  CREATE INDEX idx_account_follower_count ON account(follower_count DESC);
  ```

## 2. Follow Controller

### 2.1 Get Followers
```sql
SELECT COUNT(*) OVER() as total_count, ...
FROM account a
JOIN follow f ON a.account_id = f.follower_id
WHERE f.following_id = $1
ORDER BY f.created_at DESC
LIMIT $2 OFFSET $3
```
- **예상 실행 시간**: 🔴 > 1000ms (COUNT(*) OVER())
- **평가**: 위험
- **개선안**:
  ```sql
  -- 별도의 카운트 쿼리 또는 캐시 테이블 사용
  CREATE MATERIALIZED VIEW follower_counts AS
  SELECT following_id, COUNT(*) as count
  FROM follow
  GROUP BY following_id;
  ```

## 3. Search Controller

### 3.1 Search Tokens
```sql
SELECT COUNT(*) OVER() as total_count, ...
FROM search s
WHERE [dynamic conditions]
ORDER BY [dynamic ordering]
LIMIT $X OFFSET $Y
```
- **예상 실행 시간**: 🔴 > 2000ms
- **평가**: 위험
- **필요한 인덱스**:
  ```sql
  -- 복합 인덱스 필요
  CREATE INDEX idx_search_composite ON search(is_graduated, market_cap DESC, created_at DESC);
  CREATE INDEX idx_search_name_pattern ON search USING gin(name gin_trgm_ops);
  ```

## 4. Token Trading Controllers

### 4.1 Get Swap History
```sql
SELECT s.*, sc.total_count
FROM swap s
JOIN swap_count sc ON sc.token_id = s.token_id
WHERE s.token_id = $1
[optional filters]
ORDER BY [dynamic] DESC
LIMIT $X OFFSET $Y
```
- **예상 실행 시간**: 🟢 50-100ms
- **평가**: 최적화됨
- **이유**: swap_count 캐시 테이블 사용

### 4.2 Get Token Holders
```sql
SELECT b.balance, a.account_id, ...
FROM balance b
JOIN account a ON b.account_id = a.account_id
WHERE b.token_id = $1 AND b.balance > 0
ORDER BY b.balance DESC
LIMIT $2 OFFSET $3
```
- **예상 실행 시간**: 🟡 100-200ms
- **평가**: 개선 필요
- **필요한 인덱스**:
  ```sql
  CREATE INDEX idx_balance_token_balance ON balance(token_id, balance DESC) 
  WHERE balance > 0;
  ```

### 4.3 Chart Data
```sql
SELECT * FROM chart
WHERE token_id = $1
AND interval_type = $2
AND time_stamp >= $3
AND time_stamp <= $4
ORDER BY time_stamp ASC
LIMIT $5
```
- **예상 실행 시간**: 🟢 < 50ms
- **평가**: 최적화됨
- **필요한 인덱스**:
  ```sql
  CREATE INDEX idx_chart_lookup ON chart(token_id, interval_type, time_stamp);
  ```

## 5. Token Management

### 5.1 Get Management History
```sql
SELECT tmh.*, tmhc.total_count
FROM token_management_history tmh
JOIN token_management_history_count tmhc ON tmhc.token_id = tmh.token_id
WHERE tmh.token_id = $1
[optional filters]
ORDER BY [dynamic] DESC
```
- **예상 실행 시간**: 🟢 50-100ms
- **평가**: 최적화됨
- **이유**: 카운트 캐시 테이블 사용

## 6. New Content Controller

### 6.1 Get Latest Buy/Sell
```sql
(SELECT * FROM swap WHERE type = 'BUY' ORDER BY timestamp DESC LIMIT 1)
UNION ALL
(SELECT * FROM swap WHERE type = 'SELL' ORDER BY timestamp DESC LIMIT 1)
```
- **예상 실행 시간**: 🟡 100-200ms
- **평가**: 개선 필요
- **필요한 인덱스**:
  ```sql
  CREATE INDEX idx_swap_type_timestamp ON swap(type, timestamp DESC);
  ```

## 전체 평가 요약

### 🟢 최적화된 쿼리 (7개)
- Account CRUD 작업
- Token swap history (캐시 테이블 사용)
- Chart data 조회
- Token management history (캐시 테이블 사용)

### 🟡 개선 필요 쿼리 (5개)
- Mutual friends 조회
- Token holders 조회
- New content 조회
- Balance 관련 조회

### 🔴 위험한 쿼리 (4개)
- Followers/Following 조회 (COUNT(*) OVER())
- Search 쿼리 (동적 조건 + COUNT)
- 대량 데이터 집계 쿼리

## 권장 최적화 전략

### 1. 즉시 적용 필요
```sql
-- 1. Follow 관계 최적화
CREATE INDEX idx_follow_composite ON follow(follower_id, following_id, created_at DESC);

-- 2. Balance 조회 최적화
CREATE INDEX idx_balance_active ON balance(token_id, balance DESC) 
WHERE balance > 0;

-- 3. Swap 타입별 조회 최적화
CREATE INDEX idx_swap_type_time ON swap(type, timestamp DESC);

-- 4. Search 패턴 매칭 최적화
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX idx_search_name_trgm ON search USING gin(name gin_trgm_ops);
```

### 2. 캐시 전략
- follower/following 카운트를 Redis에 캐시
- Search 결과를 더 긴 시간(5-15초) 캐시
- Popular token의 holder 정보 사전 캐시

### 3. 쿼리 리팩토링
- COUNT(*) OVER() 제거하고 별도 카운트 쿼리 실행
- 동적 쿼리 빌더 최적화
- 불필요한 JOIN 제거

### 4. 데이터베이스 파티셔닝
```sql
-- Chart 테이블 파티셔닝 (월별)
CREATE TABLE chart_2024_01 PARTITION OF chart
FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');

-- Swap 테이블 파티셔닝 (토큰별)
CREATE TABLE swap PARTITION BY HASH (token_id);
```

## 성능 모니터링 쿼리

```sql
-- 느린 쿼리 확인
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
WHERE mean_exec_time > 100
ORDER BY mean_exec_time DESC
LIMIT 20;

-- 인덱스 사용률 확인
SELECT schemaname, tablename, indexname, idx_scan
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY schemaname, tablename;
```

## 결론

현재 상태에서 1억 건의 데이터로 운영 시:
- **약 40%의 쿼리는 즉시 사용 가능**
- **35%는 인덱스 추가로 해결 가능**
- **25%는 쿼리 구조 변경 필요**

우선순위:
1. 인덱스 추가 (1일 작업)
2. COUNT 쿼리 분리 (2일 작업)
3. 캐시 전략 강화 (1일 작업)
4. 파티셔닝 적용 (3일 작업)