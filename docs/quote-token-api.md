# Quote Token API 문서

## 개요

Quote Token API는 nad.fun에서 quote 자산으로 사용 가능한 토큰들의 메타데이터를 조회합니다.

V2부터 토큰별로 다양한 quote 자산을 지원하므로, 클라이언트가 가격 표기, 심볼 라벨,
이미지 URI 등을 일관되게 그리려면 quote token 카탈로그가 필요합니다.

데이터 소스: `quote_token` 테이블

| 컬럼 | 설명 |
|---|---|
| `quote_id` | quote 토큰 컨트랙트 주소 (PK) |
| `name` | 풀네임 (예: `"Wrapped Monad"`) |
| `symbol` | 심볼 (예: `"WMON"`) |
| `decimals` | 소수점 자리수 (기본 18) |
| `image_uri` | 토큰 아이콘 URL |

`pyth_feed_id`는 응답에서 제외됩니다 (서버 내부용).

---

## API 엔드포인트

### 1. 등록된 Quote Token 목록 조회 (`GET /quote_token`)

#### 요청
- **Method**: `GET`
- **인증**: 불필요
- **캐시**: 5분 (env `GET_QUOTE_TOKENS_RESPONSE_EXPIRATION` 으로 조정 가능, 단위: ms)

quote token은 거의 변경되지 않는 정적 데이터이므로 기본 TTL이 길게 잡혀 있습니다.

#### 응답

```json
{
  "quote_tokens": [
    {
      "quote_id": "0x5a4E0bFDeF88C9032CB4d24338C5EB3d3870BfDd",
      "name": "Wrapped Monad",
      "symbol": "WMON",
      "decimals": 18,
      "image_uri": "https://storage.nadapp.net/quote/wmon.png"
    }
  ]
}
```

`quote_tokens` 배열은 `created_at ASC` 정렬됩니다 (등록된 순서대로 반환).

#### 필드

| 필드 | 타입 | 설명 |
|---|---|---|
| `quote_id` | string | quote 토큰 컨트랙트 주소 (EIP-55) |
| `name` | string | 풀네임 |
| `symbol` | string | 심볼 (UI 라벨용 — `MarketInfo.quote_info.symbol`과 동일 값) |
| `decimals` | number | 소수점 자리수 |
| `image_uri` | string | 토큰 아이콘 URL |

#### 에러 응답
- `500`: 내부 서버 에러 (DB 조회 실패 등)

---

## TypeScript 인터페이스

```typescript
interface QuoteInfo {
  quote_id: string;
  name: string;
  symbol: string;
  decimals: number;
  image_uri: string;
}

interface QuoteTokensResponse {
  quote_tokens: QuoteInfo[];
}
```

`QuoteInfo`는 `MarketInfo.quote_info` (V2)에서 사용되는 nested 타입과 동일합니다.

---

## 사용 시나리오

- **마켓 화면**에서 토큰별 quote 표기 (예: `ATOM/WMON`) 라벨 동기화
- **차트 / 가격 표시**에서 quote 심볼과 decimals 활용
- **vault `pool_pair`** (예: `LpStats.pool_pair = "ATOM/WMON"`) 표기와 동일한 심볼 사용 보장
- **새 quote 자산 추가** 시 클라이언트는 별도 배포 없이 자동 반영
