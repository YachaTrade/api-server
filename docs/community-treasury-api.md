# 커뮤니티 트레저리 API 문서

## 개요

커뮤니티 트레저리 API는 커뮤니티 트레저리의 총 자산을 조회하는 기능을 제공합니다. 블록체인의 실시간 WMON 토큰 잔고와 데이터베이스의 바이백 금액을 합산하여 정확한 트레저리 총액을 반환합니다.

## API 엔드포인트

### 커뮤니티 트레저리 조회

```
GET /hype/community_treasury
```

**설명**: 커뮤니티 트레저리의 총 자산을 조회합니다. WMON 토큰 잔고와 바이백 금액을 합산한 값을 반환합니다.

**인증**: 불필요 (퍼블릭 엔드포인트)

**요청 예시:**
```bash
curl -X GET "https://api.example.com/hype/community_treasury" \
  -H "accept: application/json"
```

**응답 예시:**
```json
{
  "amount": "98765432100000000000000"
}
```

## 응답 스키마

### AmountResponse
| 필드명 | 타입 | 설명 |
|--------|------|------|
| `amount` | string | 총 트레저리 금액 (Wei 단위, 문자열 형태) |

## 데이터 소스

### 1. 블록체인 데이터 (WMON 잔고)
- **네트워크**: Monad Testnet
- **RPC URL**: `https://monad-testnet.api.bharvest.dev/J6UDLb2D3ewiJW584oXSLNXvCrADsTOG/`
- **WMON 컨트랙트**: `0x760AfE86e5de5fa0Ee542fc7B7B713e1c5425701`
- **트레저리 주소**: `0xDEf145d607169925fa44bb57795568Ad700b7206`
- **조회 방법**: ERC20 `balanceOf(address)` 함수 호출

### 2. 데이터베이스 (바이백 금액)
```sql
CREATE TABLE IF NOT EXISTS buyback_amount(
    epoch BIGINT NOT NULL,
    amount NUMERIC NOT NULL,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT,
    PRIMARY KEY (epoch)
);
```

### 합산 로직
```
총 트레저리 금액 = WMON 토큰 잔고 + SUM(buyback_amount.amount)
```

## 성능 최적화

### 1. 병렬 처리
```rust
let (wmon_balance_result, buyback_sum_result) = tokio::join!(
    self.get_wmon_balance(),
    self.get_buyback_amount_sum()
);
```

### 2. 싱글 플라이트 패턴
- 동일한 요청이 동시에 들어와도 하나의 실행만 수행
- 캐시 키: `"get_community_treasury"`
- 메모리 캐싱으로 중복 요청 방지

### 3. Redis 캐싱
- 캐시 키: `"community_treasury"`
- 만료 시간: 5초 (GET_COMMUNITY_TREASURY_EXPIRATION=5000)
- 블록체인 호출 부하 감소

### 4. 에러 핸들링
- **WMON 잔고 조회 실패**: 0으로 처리
- **바이백 금액 조회 실패**: 0으로 처리
- **부분 실패 허용**: 하나의 데이터 소스가 실패해도 서비스 지속

## HTTP 상태 코드

| 코드 | 설명 | 예시 상황 |
|------|------|-----------|
| 200 | 성공 | 트레저리 금액이 성공적으로 반환됨 |
| 500 | 서버 에러 | 모든 데이터 소스 실패 (매우 드문 경우) |

## 에러 응답 예시

### 500 내부 서버 에러
```json
{
  "error": "Internal Server Error",
  "message": "Failed to get community treasury"
}
```

## 사용 예시

### JavaScript (Fetch API)
```javascript
const getCommunityTreasury = async () => {
  try {
    const response = await fetch('/hype/community_treasury');
    
    if (!response.ok) {
      throw new Error(`HTTP 에러! 상태: ${response.status}`);
    }
    
    const data = await response.json();
    
    // Wei를 ETH로 변환 (18 decimal places)
    const ethAmount = parseFloat(data.amount) / Math.pow(10, 18);
    
    console.log('커뮤니티 트레저리:', {
      wei: data.amount,
      eth: ethAmount.toFixed(6)
    });
    
    return data.amount;
  } catch (error) {
    console.error('커뮤니티 트레저리 조회 에러:', error);
    throw error;
  }
};

// 사용법
getCommunityTreasury().then(amount => {
  console.log(`커뮤니티 트레저리: ${amount} Wei`);
});
```

### Python (requests + web3)
```python
import requests
from decimal import Decimal

def get_community_treasury():
    url = "https://api.example.com/hype/community_treasury"
    
    try:
        response = requests.get(url)
        response.raise_for_status()
        
        data = response.json()
        wei_amount = Decimal(data['amount'])
        
        # Wei를 ETH로 변환
        eth_amount = wei_amount / Decimal('1000000000000000000')  # 10^18
        
        return {
            'wei': str(wei_amount),
            'eth': str(eth_amount)
        }
    except requests.exceptions.RequestException as e:
        print(f"에러: {e}")
        return None

# 사용 예시
treasury = get_community_treasury()
if treasury:
    print(f"커뮤니티 트레저리: {treasury['eth']} ETH ({treasury['wei']} Wei)")
```

### cURL
```bash
# 기본 요청
curl -X GET "https://api.example.com/hype/community_treasury" \
  -H "accept: application/json"

# JSON 형태로 예쁘게 출력
curl -X GET "https://api.example.com/hype/community_treasury" \
  -H "accept: application/json" | jq '.'
```

## 구현 세부사항

### 1. 아키텍처
- **컨트롤러**: `HypeController::get_community_treasury()` - 비즈니스 로직
- **핸들러**: REST API 엔드포인트 처리  
- **블록체인**: Alloy-rs를 통한 ERC20 컨트랙트 호출
- **데이터베이스**: PostgreSQL을 통한 바이백 금액 조회

### 2. 블록체인 쿼리
```rust
// IERC20 인터페이스 정의
sol! {
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
    }
}

// 컨트랙트 호출
let contract = IERC20::new(wmon_address, provider);
let balance_result = contract.balanceOf(treasury_address).call().await?;
```

### 3. 데이터베이스 쿼리
```sql
-- 바이백 금액 총합 조회
SELECT COALESCE(SUM(amount), 0) as amount 
FROM buyback_amount
```

### 4. 에러 복원력
```rust
// 각 데이터 소스 실패 시 0으로 처리
let wmon_balance = wmon_balance_result.unwrap_or_else(|e| {
    tracing::error!("WMON 잔고 조회 실패: {}", e);
    BigDecimal::from(0)
});
```

## 모니터링 및 로깅

### 성능 로깅
- 전체 실행 시간 측정
- 개별 데이터 소스 조회 시간 측정
- 캐시 히트/미스 비율

### 에러 로깅
- 블록체인 연결 실패
- 데이터베이스 쿼리 실패
- 타임아웃 발생

### 알림 기준
- 응답 시간 > 10초
- 에러율 > 10%
- 캐시 미스율 > 90%

## 보안 고려사항

1. **API 키 보호**: RPC URL에 포함된 API 키 환경변수 관리
2. **레이트 제한**: 블록체인 RPC 호출량 제한
3. **데이터 검증**: 반환된 블록체인 데이터 유효성 검사

## 제한사항

1. **블록체인 의존성**: Monad 테스트넷 상태에 따른 가용성
2. **RPC 제한**: 외부 RPC 서비스의 호출 제한
3. **정확성**: 블록체인과 DB 간의 약간의 시간차 가능
4. **확장성**: 높은 동시성에서 RPC 병목 가능

## 향후 개선사항

1. **다중 RPC**: 여러 RPC 엔드포인트를 통한 가용성 향상
2. **웹소켓**: 실시간 블록체인 이벤트 구독
3. **예측 캐싱**: 사용 패턴 기반 사전 캐싱
4. **알림 시스템**: 트레저리 잔고 변화 알림
5. **대시보드**: 실시간 트레저리 모니터링 대시보드