# 보상 추가 히스토리 API 문서

## 개요

보상 추가 히스토리 API는 사용자별 보상 추가 내역을 조회하는 기능을 제공합니다. 블록체인에서 발생한 보상 추가 이벤트들을 추적하여 사용자가 언제, 어떤 토큰에 대해, 얼마만큼의 보상을 받았는지 확인할 수 있습니다.

## API 엔드포인트

### 보상 추가 히스토리 조회

```
GET /hype/reward_add_history
```

**설명**: 특정 사용자의 보상 추가 히스토리를 페이지네이션과 함께 조회합니다.

**인증**: 필요 (세션 쿠키 인증)

**쿼리 파라미터:**
- `page` (integer): 페이지 번호 (기본값: 1)
- `limit` (integer): 페이지당 항목 수 (기본값: 20)

**요청 예시:**
```bash
curl -X GET "https://api.example.com/hype/reward_add_history?page=1&limit=10" \
  -H "accept: application/json" \
  -H "Cookie: session=your_session_cookie"
```

**응답 예시:**
```json
{
  "history": [
    {
      "epoch": 1234567890,
      "token_info": {
        "token_id": "0x1234567890123456789012345678901234567890ab",
        "name": "Example Token",
        "symbol": "EXT",
        "image_uri": "https://example.com/token.png",
        "created_at": 1698765432,
        "description": "Example token description"
      },
      "amount": "1000000000000000000",
      "total_amount": "5000000000000000000",
      "created_at": 1698765432,
      "transaction_hash": "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    }
  ],
  "total_count": 25
}
```

## 응답 스키마

### HypeRewardAddHistoryResponse
| 필드명 | 타입 | 설명 |
|--------|------|------|
| `history` | Array[RewardAdd] | 보상 추가 히스토리 목록 |
| `total_count` | integer | 전체 항목 수 |

### RewardAdd
| 필드명 | 타입 | 설명 |
|--------|------|------|
| `epoch` | integer | 에포크 번호 |
| `token_info` | TokenInfoWithCreatedAtAndDescription | 토큰 정보 |
| `amount` | string | 개별 보상 금액 (Wei 단위) |
| `total_amount` | string | 누적 보상 금액 (Wei 단위) |
| `created_at` | integer | 생성 시간 (Unix 타임스탬프) |
| `transaction_hash` | string | 트랜잭션 해시 |

## 데이터 소스

### 데이터베이스 테이블
```sql
CREATE TABLE IF NOT EXISTS reward_add_history(
    epoch BIGINT NOT NULL,
    token_id VARCHAR(42) NOT NULL REFERENCES token(token_id),
    account_id VARCHAR(42) NOT NULL REFERENCES account(account_id),
    amount NUMERIC NOT NULL,
    total_amount NUMERIC NOT NULL DEFAULT 0,
    transaction_hash VARCHAR(66) NOT NULL,
    log_index BIGINT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT,
    PRIMARY KEY (epoch, account_id, token_id, transaction_hash, log_index)
);
```

### 카운트 테이블
```sql
CREATE TABLE IF NOT EXISTS reward_add_history_count(
    account_id VARCHAR(42) PRIMARY KEY REFERENCES account(account_id),
    total_count BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT
);
```

## 성능 최적화

### 1. 싱글 플라이트 패턴
- 동일한 요청이 동시에 들어와도 하나의 쿼리만 실행
- 캐시 키: `"hype_reward_add_history:{account_id}:{page}:{limit}"`
- 메모리 캐싱으로 중복 요청 방지

### 2. Redis 캐싱
- 캐시 키: `"hype_reward_add_history:{account_id}:{page}:{limit}"`
- 만료 시간: 5초 (GET_REWARD_ADD_HISTORY_EXPIRATION=5000)
- 빈번한 조회에 대한 응답 성능 향상

### 3. 데이터베이스 최적화
- 병렬 쿼리 실행 (데이터 조회 + 카운트 조회)
- 카운트 테이블 활용으로 COUNT 쿼리 성능 향상
- 1초 쿼리 타임아웃 설정
- PostgreSQL 읽기 전용 풀 사용

### 4. 자동 카운트 업데이트
```sql
-- INSERT 시 자동으로 카운트 증가
CREATE TRIGGER trigger_update_reward_add_history_count
    AFTER INSERT ON reward_add_history
    FOR EACH ROW
    EXECUTE FUNCTION update_reward_add_history_count();
```

## HTTP 상태 코드

| 코드 | 설명 | 예시 상황 |
|------|------|-----------|
| 200 | 성공 | 보상 추가 히스토리가 성공적으로 반환됨 |
| 400 | 잘못된 요청 | 페이지네이션 파라미터 오류 |
| 401 | 인증 실패 | 세션 쿠키 없음 또는 만료됨 |
| 500 | 서버 에러 | 데이터베이스 연결 실패, 쿼리 타임아웃 |

## 에러 응답 예시

### 401 인증 실패
```json
{
  "error": "Unauthorized",
  "message": "Authentication required"
}
```

### 500 내부 서버 에러
```json
{
  "error": "Internal Server Error",
  "message": "Failed to get hype reward add history"
}
```

## 사용 예시

### JavaScript (Fetch API)
```javascript
const getRewardAddHistory = async (page = 1, limit = 20) => {
  try {
    const response = await fetch(`/hype/reward_add_history?page=${page}&limit=${limit}`, {
      method: 'GET',
      credentials: 'include', // 쿠키 포함
      headers: {
        'Accept': 'application/json'
      }
    });
    
    if (!response.ok) {
      throw new Error(`HTTP 에러! 상태: ${response.status}`);
    }
    
    const data = await response.json();
    console.log('보상 추가 히스토리:', data);
    return data;
  } catch (error) {
    console.error('보상 추가 히스토리 조회 에러:', error);
    throw error;
  }
};

// 사용법
getRewardAddHistory(1, 10).then(history => {
  console.log(`총 ${history.total_count}개의 보상 추가 기록 중 ${history.history.length}개 조회됨`);
  
  history.history.forEach(record => {
    const ethAmount = parseFloat(record.amount) / Math.pow(10, 18);
    console.log(`${record.token_info.name}: ${ethAmount.toFixed(6)} ETH`);
  });
});
```

### Python (requests)
```python
import requests
from decimal import Decimal

def get_reward_add_history(page=1, limit=20, session_cookie=None):
    url = f"https://api.example.com/hype/reward_add_history?page={page}&limit={limit}"
    
    cookies = {'session': session_cookie} if session_cookie else None
    
    try:
        response = requests.get(url, cookies=cookies)
        response.raise_for_status()
        
        data = response.json()
        return data
    except requests.exceptions.RequestException as e:
        print(f"에러: {e}")
        return None

def format_amount(wei_amount):
    """Wei를 ETH로 변환"""
    wei = Decimal(wei_amount)
    eth = wei / Decimal('1000000000000000000')
    return str(eth)

# 사용 예시
history = get_reward_add_history(page=1, limit=10, session_cookie="your_session_cookie")
if history:
    print(f"총 {history['total_count']}개의 기록")
    
    for record in history['history']:
        eth_amount = format_amount(record['amount'])
        print(f"{record['token_info']['name']}: {eth_amount} ETH")
        print(f"트랜잭션: {record['transaction_hash']}")
        print("---")
```

### cURL
```bash
# 기본 요청 (페이지 1, 20개 항목)
curl -X GET "https://api.example.com/hype/reward_add_history?page=1&limit=20" \
  -H "accept: application/json" \
  -H "Cookie: session=your_session_cookie"

# 특정 페이지 요청
curl -X GET "https://api.example.com/hype/reward_add_history?page=2&limit=10" \
  -H "accept: application/json" \
  -H "Cookie: session=your_session_cookie" | jq '.'
```

## 구현 세부사항

### 1. 아키텍처
- **컨트롤러**: `HypeController::get_hype_reward_add_history()` - 비즈니스 로직
- **핸들러**: REST API 엔드포인트 처리 및 인증 확인
- **캐시**: 싱글 플라이트 + Redis 이중 캐싱

### 2. 데이터베이스 쿼리
```sql
-- 보상 추가 히스토리 조회
SELECT 
    rah.epoch,
    rah.token_id,
    rah.amount,
    rah.total_amount,
    rah.transaction_hash,
    rah.created_at,
    t.name,
    t.symbol,
    t.image_uri,
    t.created_at as token_created_at
FROM reward_add_history rah
JOIN token t ON rah.token_id = t.token_id
WHERE rah.account_id = $1
ORDER BY rah.created_at DESC
LIMIT $2 OFFSET $3
```

### 3. 캐싱 전략
```rust
// 싱글 플라이트 패턴
with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
    // 데이터베이스 쿼리 실행
})

// Redis 캐싱
redis.get_hype_reward_add_history_response(&account_id, &params).await  // 캐시 조회
redis.set_hype_reward_add_history_response(&account_id, &params, &response).await  // 캐시 저장
```

## 모니터링 및 로깅

### 성능 로깅
- 모든 요청에 대한 실행 시간 측정
- 캐시 히트/미스 비율 모니터링
- 데이터베이스 쿼리 성능 추적

### 에러 로깅
```rust
tracing::error!("보상 추가 히스토리 조회 실패: {}", err);
```

## 보안 고려사항

1. **인증 검증**: 세션 쿠키를 통한 사용자 인증 필수
2. **개인정보 보호**: 본인의 보상 히스토리만 조회 가능
3. **레이트 제한**: 과도한 요청 방지를 위한 캐싱 적용
4. **데이터 검증**: 모든 입력 파라미터 유효성 검사

## 제한사항

1. **캐시 일관성**: 5초간의 캐시로 인해 실시간 데이터와 약간의 지연 발생 가능
2. **페이지네이션 제한**: 대량의 데이터 조회 시 성능 고려 필요
3. **인증 의존성**: 세션 기반 인증에 의존

## 향후 개선사항

1. **실시간 업데이트**: WebSocket을 통한 실시간 보상 알림
2. **필터링 기능**: 토큰별, 기간별 필터링 옵션 추가
3. **집계 정보**: 일별, 월별 보상 집계 정보 제공
4. **내보내기 기능**: CSV, Excel 형태로 데이터 내보내기
5. **통계 정보**: 평균 보상, 최대 보상 등 통계 데이터 제공