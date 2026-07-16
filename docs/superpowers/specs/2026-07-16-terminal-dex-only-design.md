# Terminal API DEX 전용 조회 설계

- **날짜**: 2026-07-16
- **기준 브랜치**: `v2`
- **상태**: 설계 확정 (구현 대기)
- **관련 코드**: `src/controllers/terminal/mod.rs`, `src/services/terminal/mod.rs`
- **관련 문서**: `docs/terminal-api.md`

## 1. 배경과 목표

Terminal API는 현재 `/pair`에서 `CURVE`, `DEX`, `V2_CURVE`, `V2_DEX` 시장을 모두 조회할 수 있다. `/events`의 swap 조회는 `V2_CURVE`만 제외하므로 `CURVE`, `DEX`, `V2_DEX` 이벤트를 반환하고, mint/burn 조회에는 명시적인 시장 유형 제한이 없다.

외부 소비자는 Terminal API에서 DEX에 상장된 시장 데이터만 필요하다. 따라서 시장 데이터 엔드포인트인 `/pair`와 `/events`가 `DEX`, `V2_DEX`만 반환하도록 SQL 조회 단계에 명시적인 allowlist를 적용한다. Curve 데이터는 조회 결과에 포함시키지 않는다.

## 2. 범위

### 포함

- `GET /pair?id=<token_address>`
  - 토큰의 현재 `market.market_type`이 `DEX` 또는 `V2_DEX`일 때만 페어를 반환한다.
  - 현재 시장이 `CURVE` 또는 `V2_CURVE`이면 기존 pair not-found 경로로 처리한다.
- `GET /events?fromBlock=<n>&toBlock=<n>`
  - swap, mint(join), burn(exit) SQL 조회에서 `DEX`, `V2_DEX`만 허용한다.
  - `CURVE`, `V2_CURVE`, 알 수 없는 시장 유형은 반환하지 않는다.
- 허용/제외 동작을 검증하는 테스트 추가 및 기존 테스트 갱신.
- `docs/terminal-api.md`를 실제 DEX 전용 노출 정책에 맞게 갱신.

### 제외

- `GET /asset?id=<token_address>` 변경.
- `GET /:token_address` 메타데이터 조회 변경.
- `GET /latest-block` 변경.
- 요청 파라미터, `token_address` 사용 방식 또는 주소 정규화 정책 변경.
- 응답 구조, 필드 값 산정, 이벤트 정렬, 블록 범위 검증 변경.
- 데이터베이스 스키마 또는 데이터 마이그레이션.

## 3. 설계 결정

### 3.1 SQL allowlist

필터링은 서비스에서 조회 후 수행하지 않고 controller의 SQL 조건에 직접 둔다. 허용 대상은 부정 조건이 아니라 다음의 명시적인 allowlist로 표현한다.

```sql
IN ('DEX', 'V2_DEX')
```

이 방식은 Curve 행을 데이터베이스에서 애플리케이션으로 읽어오지 않으며, 새로운 `market_type`이 추가되더라도 의도적인 코드 변경 전에는 자동 노출되지 않는다.

### 3.2 `/pair`

`TerminalController::get_pair`의 기존 `market m` join을 이용해 다음 조건을 추가한다.

```sql
WHERE t.token_id = $1
  AND m.market_type IN ('DEX', 'V2_DEX')
```

`id`는 계속 토큰 주소다. 허용 시장에서는 기존 `PairRow` 변환, `pairId`, `dexKey`, asset 정렬, fee 계산을 그대로 사용한다. Curve 시장은 쿼리 결과가 없으므로 `TerminalService::get_pair`의 기존 not-found 응답으로 이어진다. 별도의 Curve 전용 에러나 응답 필드는 추가하지 않는다.

`get_pair_by_pool_id`는 현재 공개 `/pair` 요청 경로에서 사용되지 않으며 이번 범위에 포함하지 않는다. 향후 공개 경로에서 사용하게 되면 동일한 DEX allowlist가 필요하다.

### 3.3 `/events`

세 이벤트 소스의 쿼리에 각각 allowlist를 적용한다.

- swap: 역사적 이벤트 자체의 venue를 보존하는 `s.market_type IN ('DEX', 'V2_DEX')`를 사용한다. 토큰의 현재 `market.market_type`으로 과거 swap을 재분류하지 않는다.
- mint(join): 기존 `market m` join을 사용해 `m.market_type IN ('DEX', 'V2_DEX')`를 적용한다.
- burn(exit): 기존 `market m` join을 사용해 `m.market_type IN ('DEX', 'V2_DEX')`를 적용한다.

세 쿼리는 계속 병렬 실행한다. 각 쿼리의 블록 범위 조건과 내부 정렬은 유지하고, 합쳐진 이벤트의 최종 정렬도 기존 `(block_number, tx_index, log_index)` 오름차순을 유지한다. 따라서 반환되는 이벤트 집합만 축소되고 응답 순서와 JSON 스키마는 바뀌지 않는다.

## 4. API 동작과 오류 처리

| 엔드포인트 | DEX / V2_DEX | CURVE / V2_CURVE | 기타 동작 |
|---|---|---|---|
| `GET /pair` | 기존 pair 응답 | 기존 HTTP 404 not-found 응답 | 스키마와 값 산정 유지 |
| `GET /events` | 해당 범위 이벤트 반환 | 이벤트 목록에서 제외 | 빈 결과는 `events: []` |
| `GET /asset` | 기존 동작 | 기존 동작 | 변경 없음 |
| `GET /:token_address` | 기존 동작 | 기존 동작 | 변경 없음 |
| `GET /latest-block` | 기존 동작 | 해당 없음 | 변경 없음 |

SQL 실행 실패의 오류 매핑, 잘못된 이벤트 블록 범위의 검증 오류, 인증 및 라우팅은 기존 동작을 유지한다.

## 5. 테스트 전략

`src/controllers/terminal/mod.rs`의 기존 SQLx 테스트를 중심으로 다음을 검증한다.

1. `/pair` 조회 기반 controller 테스트
   - `DEX`, `V2_DEX` 시장은 조회된다.
   - `CURVE`, `V2_CURVE` 시장은 조회되지 않는다.
2. swap 이벤트 테스트
   - `DEX`, `V2_DEX` swap만 반환된다.
   - `CURVE`, `V2_CURVE` swap은 같은 블록 범위에 있어도 제외된다.
   - 필터 기준이 현재 market이 아니라 `swap.market_type`임을 검증한다.
3. mint/burn 이벤트 테스트
   - joined market이 `DEX`, `V2_DEX`인 행만 반환된다.
   - Curve 또는 알 수 없는 시장 유형의 행은 제외된다.
4. 회귀 검증
   - 여러 이벤트 유형을 합친 후 기존 정렬이 유지된다.
   - DEX/V2_DEX의 pair 및 event 응답 스키마와 필드 산정이 바뀌지 않는다.
   - `/asset`, `/:token_address`, `/latest-block` 관련 코드는 수정하지 않는다.

구현 후 terminal 관련 단위/SQLx 테스트를 실행하고, 가능한 범위에서 전체 테스트와 lint를 수행한다.

## 6. 문서와 배포

`docs/terminal-api.md`에서 다음 내용을 명시한다.

- `/pair`는 현재 시장이 `DEX` 또는 `V2_DEX`인 토큰만 제공한다.
- `/events`는 swap/join/exit 모두 `DEX`, `V2_DEX` 이벤트만 제공한다.
- `/asset`, `/:token_address`, `/latest-block`은 시장 유형과 무관하게 기존 동작을 유지한다.

스키마 변경과 마이그레이션이 없으므로 일반 애플리케이션 배포만 필요하다. 기존 Curve 소비자는 `/pair`에서 not-found를 받고 `/events`에서 Curve 이벤트를 더 이상 받지 않으므로, 이는 의도된 동작 변경으로 릴리스 문서에 기록한다.

## 7. 완료 기준

- `/pair` SQL이 `DEX`, `V2_DEX`만 허용한다.
- `/events`의 swap, mint, burn SQL이 `DEX`, `V2_DEX`만 허용한다.
- Curve 및 알 수 없는 시장 유형이 `/pair`와 `/events` 응답에 노출되지 않는다.
- 응답 스키마, DEX/V2_DEX 값 산정, 이벤트 정렬은 기존과 동일하다.
- 비대상 엔드포인트는 변경되지 않는다.
- 테스트와 `docs/terminal-api.md`가 새 정책을 반영한다.
- 마이그레이션 파일과 `migrations` gitlink는 변경하지 않는다.
