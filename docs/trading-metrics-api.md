# Trading Metrics API 문서

## 개요

Trading Metrics API는 토큰의 거래 통계 데이터를 다양한 시간 프레임으로 조회할 수 있는 기능을 제공합니다. 거래 횟수, 거래량, 가격 변화율 등의 정보를 실시간으로 제공합니다.

## API 엔드포인트

### 1. 단일 시간 프레임 조회

```
GET /trade/metrics/{token_id}
```

**설명**: 특정 토큰의 단일 시간 프레임에 대한 거래 통계를 조회합니다.

**Parameters:**
- `token_id` (Path, required): 조회할 토큰의 ID (EVM 주소 형식)
- `timeframe` (Query, required): 시간 프레임

**시간 프레임 옵션:**
- `1`, `5`, `15`, `30`, `60`: 분 단위
- `4H`: 4시간
- `D`: 1일
- `W`: 1주
- `M`: 1개월

**요청 예시:**
```bash
GET /trade/metrics/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframe=D
```

**응답 예시:**
```json
{
  "token_id": "0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589",
  "buy_count": 150,
  "sell_count": 89,
  "volume": "1234567890123456789",
  "timeframe": "1d",
  "price_change_percent": "5.67",
  "current_price": "0.001234",
  "start_price": "0.001167"
}
```

### 2. 다중 시간 프레임 배치 조회

```
GET /trade/metrics-batch/{token_id}
```

**설명**: 특정 토큰의 여러 시간 프레임에 대한 거래 통계를 한 번에 조회합니다.

**Parameters:**
- `token_id` (Path, required): 조회할 토큰의 ID
- `timeframes` (Query, required): 쉼표로 구분된 시간 프레임 문자열

**요청 예시:**
```bash
GET /trade/metrics-batch/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframes=D,1H,5,15
```

**응답 예시:**
```json
[
  {
    "token_id": "0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589",
    "buy_count": 150,
    "sell_count": 89,
    "volume": "1234567890123456789",
    "timeframe": "1d",
    "price_change_percent": "5.67",
    "current_price": "0.001234",
    "start_price": "0.001167"
  },
  {
    "token_id": "0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589",
    "buy_count": 25,
    "sell_count": 18,
    "volume": "234567890123456789",
    "timeframe": "1h",
    "price_change_percent": "2.34",
    "current_price": "0.001234",
    "start_price": "0.001206"
  }
]
```

## 응답 필드

| 필드명 | 타입 | 설명 |
|--------|------|------|
| `token_id` | string | 토큰 ID |
| `buy_count` | number | 매수 거래 횟수 |
| `sell_count` | number | 매도 거래 횟수 |
| `volume` | string | 총 거래량 (native token 기준, Wei 단위) |
| `timeframe` | string | 시간 프레임 표시 문자열 (예: "1d", "1h") |
| `price_change_percent` | string | 가격 변화율 (%) |
| `current_price` | string\|null | 현재가 |
| `start_price` | string\|null | 시작가 |

## 가격 데이터 처리

### 1. 우선순위
1. **Chart 테이블**: OHLC 데이터에서 정확한 가격 정보 조회
2. **Fallback**: Chart 데이터 없을 시 0% 변화율 반환

### 2. 가격 변화율 계산
- **공식**: `((현재가 - 시작가) / 시작가) × 100`
- **시작가**: 해당 시간 프레임 첫 번째 캔들의 `open_price`
- **현재가**: 최신 캔들의 `close_price`
- **데이터 없음**: `"0.00"` 반환

## 성능 최적화

### 1. 병렬 처리
- Batch API는 각 시간 프레임을 `tokio::spawn`으로 동시 처리
- 여러 시간 프레임 조회 시 성능 향상

### 2. 데이터베이스 최적화
- PostgreSQL 읽기 전용 풀 사용
- 1초 쿼리 타임아웃 설정
- 단일 쿼리로 시작가/현재가 동시 조회 (윈도우 함수 활용)

### 3. 로깅 및 모니터링
- 모든 요청에 대한 성능 로깅
- 100ms 이상 소요 시 경고 로그
- 상세한 에러 메시지 제공

## 에러 처리

### HTTP 상태 코드

| 코드 | 설명 | 예시 |
|------|------|------|
| 200 | 성공 | 정상적인 데이터 반환 |
| 400 | 잘못된 요청 | 유효하지 않은 토큰 ID, 시간 프레임 |
| 500 | 서버 에러 | 데이터베이스 연결 실패, 쿼리 타임아웃 |

### 에러 응답 형식
```json
{
  "error": "Bad Request",
  "message": "Invalid token ID"
}
```

## 사용 예시

### cURL
```bash
# 단일 시간 프레임
curl -X GET "http://localhost:8000/trade/metrics/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframe=D" \
  -H "accept: application/json"

# 다중 시간 프레임
curl -X GET "http://localhost:8000/trade/metrics-batch/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframes=D,1H,5,15" \
  -H "accept: application/json"
```

### JavaScript
```javascript
// 단일 시간 프레임
const response = await fetch('/trade/metrics/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframe=D');
const metrics = await response.json();

// 다중 시간 프레임
const batchResponse = await fetch('/trade/metrics-batch/0x25eD53F8A8Ead909F82e6F41F0480BE8ceF24589?timeframes=D,1H,5');
const batchMetrics = await batchResponse.json();
```

## 구현 세부사항

### 1. 아키텍처
- **Controller**: `MetricsController` - 비즈니스 로직
- **Handler**: REST API 엔드포인트 처리
- **Types**: 타입 정의 및 직렬화/역직렬화

### 2. 데이터베이스 쿼리
```sql
-- 거래 통계 조회
SELECT 
    COUNT(*) as total_count,
    SUM(CASE WHEN is_buy = true THEN 1 ELSE 0 END) as buy_count,
    SUM(CASE WHEN is_buy = false THEN 1 ELSE 0 END) as sell_count,
    COALESCE(SUM(native_amount), 0) as volume
FROM swap 
WHERE token_id = $1 AND created_at > $2

-- 가격 데이터 조회 (단일 쿼리 최적화)
WITH price_data AS (
    SELECT open_price, close_price, time_stamp,
           ROW_NUMBER() OVER (ORDER BY time_stamp ASC) as first_row,
           ROW_NUMBER() OVER (ORDER BY time_stamp DESC) as last_row
    FROM chart 
    WHERE token_id = $1 AND interval_type = $2 AND time_stamp >= $3
)
SELECT 
    FIRST_VALUE(open_price) OVER (ORDER BY first_row) as start_price,
    FIRST_VALUE(close_price) OVER (ORDER BY last_row) as current_price
FROM price_data 
WHERE first_row = 1 OR last_row = 1
LIMIT 1
```

### 3. 시간 프레임 매핑
- API → Chart 테이블: `1` → `"1"`, `D` → `"D"`, `4H` → `"4H"`
- 표시용 문자열: `D` → `"1d"`, `4H` → `"4h"`, `1` → `"1m"`

## 주의사항

1. **토큰 ID 검증**: 유효한 EVM 주소 형식인지 확인
2. **시간 프레임**: 대소문자 구분 (`D`, `W`, `M`은 대문자)
3. **배치 요청**: 너무 많은 시간 프레임 요청 시 성능 저하 가능
4. **가격 데이터**: Chart 테이블이 비어있을 경우 0% 변화율 반환

## 향후 개선사항

1. **캐싱**: Redis 캐싱으로 성능 향상
2. **Rate Limiting**: API 호출 제한
3. **추가 메트릭**: 거래자 수, 평균 거래 크기 등
4. **실시간 업데이트**: WebSocket 지원