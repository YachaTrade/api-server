# 총 소비 포인트 API 문서

## 개요

총 소비 포인트 API는 시스템 전체에서 소비된 포인트의 총합을 조회하는 기능을 제공합니다. 모든 사용자가 하이프 보드에서 소비한 포인트의 누적 합계를 실시간으로 확인할 수 있습니다.

## API 엔드포인트

### 총 소비 포인트 조회

```
GET /hype/total_spend_point
```

**설명**: 시스템 전체에서 소비된 포인트의 총합을 조회합니다.

**인증**: 불필요 (퍼블릭 엔드포인트)

**요청 예시:**
```bash
curl -X GET "https://api.example.com/hype/total_spend_point" \
  -H "accept: application/json"
```

**응답 예시:**
```json
{
  "amount": "12345678900"
}
```

## 응답 스키마

### AmountResponse
| 필드명 | 타입 | 설명 |
|--------|------|------|
| `amount` | string | 총 소비 포인트 (문자열 형태의 숫자) |

## 데이터 소스

### 데이터베이스 테이블
```sql
CREATE TABLE IF NOT EXISTS total_spent_point(
    id INTEGER PRIMARY KEY DEFAULT 1,
    spend_point BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT single_row CHECK (id = 1)
);
```

### 업데이트 메커니즘
- **자동 업데이트**: PostgreSQL 트리거를 통해 `point` 테이블의 `spend_point` 변경 시 자동 누적
- **실시간 반영**: 사용자가 포인트를 소비할 때마다 즉시 업데이트

## 성능 최적화

### 1. 싱글 플라이트 패턴
- 동일한 요청이 동시에 들어와도 하나의 쿼리만 실행
- 캐시 키: `"get_total_spend_point"`
- 캐시 TTL: 1초 (1000ms)

### 2. Redis 캐싱
- 캐시 키: `"total_spend_point"`
- 만료 시간: 1초 (GET_TOTAL_SPEND_POINT_EXPIRATION=1000)
- 캐시 우선 조회로 데이터베이스 부하 감소

### 3. 데이터베이스 최적화
- 단일 행 테이블로 빠른 조회
- 1초 쿼리 타임아웃 설정
- PostgreSQL 읽기 전용 풀 사용

## HTTP 상태 코드

| 코드 | 설명 | 예시 상황 |
|------|------|-----------
| 200 | 성공 | 총 소비 포인트가 성공적으로 반환됨 |
| 500 | 서버 에러 | 데이터베이스 연결 실패, 쿼리 타임아웃 |

## 에러 응답 예시

### 500 내부 서버 에러
```json
{
  "error": "Internal Server Error",
  "message": "Failed to get total spend point"
}
```

## 사용 예시

### JavaScript (Fetch API)
```javascript
const getTotalSpendPoint = async () => {
  try {
    const response = await fetch('/hype/total_spend_point');
    
    if (!response.ok) {
      throw new Error(`HTTP 에러! 상태: ${response.status}`);
    }
    
    const data = await response.json();
    console.log('총 소비 포인트:', data.amount);
    return data.amount;
  } catch (error) {
    console.error('총 소비 포인트 조회 에러:', error);
    throw error;
  }
};

// 사용법
getTotalSpendPoint().then(amount => {
  console.log(`총 소비 포인트: ${amount}`);
});
```

### Python (requests)
```python
import requests

def get_total_spend_point():
    url = "https://api.example.com/hype/total_spend_point"
    
    try:
        response = requests.get(url)
        response.raise_for_status()
        
        data = response.json()
        return data['amount']
    except requests.exceptions.RequestException as e:
        print(f"에러: {e}")
        return None

# 사용 예시
total_amount = get_total_spend_point()
if total_amount:
    print(f"총 소비 포인트: {total_amount}")
```

### cURL
```bash
# 기본 요청
curl -X GET "https://api.example.com/hype/total_spend_point" \
  -H "accept: application/json"

# JSON 형태로 예쁘게 출력
curl -X GET "https://api.example.com/hype/total_spend_point" \
  -H "accept: application/json" | jq '.'
```

## 구현 세부사항

### 1. 아키텍처
- **컨트롤러**: `HypeController::get_total_spend_point()` - 비즈니스 로직
- **핸들러**: REST API 엔드포인트 처리
- **캐시**: 싱글 플라이트 + Redis 이중 캐싱

### 2. 데이터베이스 쿼리
```sql
-- 실제 실행되는 쿼리
SELECT spend_point::NUMERIC as amount 
FROM total_spent_point 
WHERE id = 1
```

### 3. 캐싱 전략
```rust
// 싱글 플라이트 패턴
with_cache(&GLOBAL_CACHE.cache, &"get_total_spend_point", || {
    // 데이터베이스 쿼리 실행
})

// Redis 캐싱
redis.get_total_spend_point_response().await  // 캐시 조회
redis.set_total_spend_point_response().await  // 캐시 저장
```

## 모니터링 및 로깅

### 성능 로깅
- 모든 요청에 대한 실행 시간 측정
- 캐시 히트/미스 로깅
- 데이터베이스 쿼리 성능 모니터링

### 에러 로깅
```rust
tracing::error!("총 소비 포인트 조회 실패: {}", err);
```

## 제한사항

1. **캐시 일관성**: 1초간의 캐시로 인해 실시간 데이터와 약간의 지연 발생 가능
2. **단일 장애점**: `total_spent_point` 테이블이 손상되면 서비스 불가
3. **확장성**: 매우 높은 TPS에서는 추가 최적화 필요

## 향후 개선사항

1. **분산 캐싱**: 여러 인스턴스 간 캐시 동기화
2. **실시간 업데이트**: WebSocket을 통한 실시간 데이터 푸시
3. **백업 전략**: 데이터 손실 방지를 위한 백업 메커니즘
4. **메트릭 수집**: Prometheus/Grafana를 통한 모니터링 강화