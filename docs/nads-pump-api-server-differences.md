# NADS Pump API Server 대비 GIWA 변경사항

작성일: 2026-07-22

이 문서는 NADS Pump API의 2026-07-20 기준선(`a2e67aa`)과 GIWA 정리 작업 트리를 실제 라우터, 공개 타입, SQL 쿼리, 설정 및 `giwa/migrations` 기준으로 비교한다.

## 요약

GIWA는 NADS의 다중 토큰 세대와 참여형 제품 API를 그대로 가져오지 않는다. 공개 토큰 모델은 단일 세대이며 시장은 `CURVE`, `DEX`만 지원한다.

| 영역 | NADS Pump | GIWA |
| --- | --- | --- |
| 토큰 세대 | `V1`, `V2` 분기 | 버전 개념 없음 |
| 시장 타입 | `CURVE`, `DEX`, `V2_CURVE`, `V2_DEX` | `CURVE`, `DEX` |
| 생성 설정 | 세대별 bonding curve/token implementation | `BONDING_CURVE`, `TOKEN_IMPL` |
| `MarketInfo` | `fee_info` 포함 | `fee_info`, `FeeInfo` 제거 |
| 참여형 제품 | Hype, Point, Chester, Raffle, Reward | 제거 |
| NADS 최신 제품 | X verification, DevPost, DEX 관리, Vault, Dividend, Quote Token API | 제거 |
| 마이그레이션 | 번호별/upgrade migration | fresh DB용 `0001_init.sql` |

## GIWA에 남는 상위 라우터

현재 `src/main.rs`에 등록되는 상위 라우터는 17개다.

- `auth`, `account`, `token`, `search`, `trade`, `profile`, `order`
- `new_event`, `metadata`, `metrics`, `terminal`, `trend`
- `leaderboard`, `api_key`, `cms`, `agent`, `health`

`leaderboard`는 PnL만 제공하고, `profile`에는 기본 프로필·보유 토큰·생성 토큰·swap history만 남는다.

## 제거된 라우터와 관련 코드

다음 제품은 HTTP 등록뿐 아니라 router/controller/service/type/OpenAPI 경로와 전용 캐시 및 설정까지 제거된다.

| 제품 | 대표 제거 경로 |
| --- | --- |
| Hype/Point/Reward | `/hype/*`, `/profile/point-history`, `/leaderboard/hype_point` |
| Chester | `/chester/*`, `/cms/analytics/chester-retention` |
| Raffle | `/raffle/*` |
| X verification | `/x/oauth/*`, `/x/followed-by*`, `/x/verification/*`, `/trade/xinfo/*` |
| DevPost | `/dev-post/*` |
| DEX 관리 API | `/dex/*` |
| Vault/Gift fee | `/vault/*`, `/profile/gift-fee/*` |
| Dividend | `/dividend/*`와 profile/trade 연계 경로 |
| Quote Token 관리 API | `/quote_token` |

`quote_token`, `pool`, LP 관련 테이블은 남은 token/trading/terminal 내부 조회에서 사용되므로 공개 관리 라우터 삭제와 별개로 유지한다.

## 공개 API 계약 변경

### 토큰 버전 제거

- `TokenVersion` 타입 제거
- `TokenInfo.version` 제거
- `MineSaltRequest.version` 제거
- token/search/order/profile/trend/new-event 응답의 버전 조회 및 분기 제거
- Salt 계산은 `BONDING_CURVE`, `TOKEN_IMPL` 한 쌍만 사용

클라이언트는 salt 요청에 `version`을 보내지 않아야 하며 응답에서도 토큰 버전을 기대하지 않아야 한다.

### 시장 타입 축소

Rust의 `MarketType`은 `Curve`, `Dex`만 가진다. JSON과 DB 값은 각각 `CURVE`, `DEX`이며 `V2_CURVE`, `V2_DEX`는 허용하지 않는다.

빈 Curve market의 `market_id`는 세대별 분기 없이 `BONDING_CURVE`로 보정한다.

### `MarketInfo.fee_info` 제거

- `FeeInfo` 타입 제거
- 모든 `MarketInfo` 응답에서 `fee_info` 제거
- `fee_config` JOIN과 세대별 fee 조립 제거
- 관련 OpenAPI와 API 문서 예시 제거

Terminal 호환 응답의 `pair.feeBps`는 `MarketInfo.fee_info`와 별도 계약이다. `fee_config` 제거 후 총 수수료를 정확히 계산할 근거가 없으므로 GIWA는 이 optional 필드를 생략한다.

## Terminal API

삭제 대상이 아닌 최신 안정화는 유지한다.

- `/pair`는 `/events`가 반환한 DEX pool address를 `id`로 받는다.
- `/pair`, `/events`는 `DEX` market만 노출한다.
- token과 `market.quote_id`를 주소 순으로 정렬해 asset0/asset1과 reserve 방향을 결정한다.
- `/asset`은 온체인 ERC-20 metadata/totalSupply를 우선하고, DB의 `token`, `quote_token`을 폴백으로 사용한다.
- 삭제된 `whitelist_token`, `fee_config`에는 의존하지 않는다.
- DEX `dexKey`는 `nadswap`이며, 근거 없는 수수료 추정을 피하기 위해 `feeBps`는 생략한다.

## 데이터베이스와 migrations

API submodule remote는 `YachaTrade/migrations`다. canonical schema 정리는 `giwa/migrations` 커밋 `9694aa7`에 기록됐고, 현재 API 작업 트리의 gitlink도 해당 커밋을 가리킨다.

기존 GIWA 정리에서 이미 제거된 항목:

- Hype, Point, Vote, Reward, Chester, Raffle 테이블·함수·트리거
- `token.version`, `token.chain`
- `account_x`, account verification
- creator reward/treasury 및 unused whitelist/DEX helper tables
- `market.market_type`, `swap.market_type`의 V2 값

이번 추가 정리에서 제거되는 항목:

- `token_x_verification`, `token_x_followed_by`, `token_x_reservation`
- DevPost sequence/table/index 전체
- Dividend event/aggregate schema 전체
- Vault/Gift/Creator fee vault event/aggregate schema 전체
- Gift bot의 `gift_tweet` 큐·알림 트리거
- 사용하지 않는 fee collection/settlement/to-claim event tables

남는 핵심 스키마는 `market`, `swap`, `quote_token`, `pool`, `lp_position`, `mint`, `burn`과 retained API가 사용하는 `api_keys`, `account_activity` 등이다. `api_keys.id`는 pgactive 없이 PostgreSQL identity를 사용한다.

## 클라이언트 영향

- `TokenInfo.version`, salt request `version`, `MarketInfo.fee_info` 의존을 제거한다.
- market type switch는 `CURVE`, `DEX` 두 값만 처리한다.
- 삭제된 제품 화면과 API 호출을 GIWA에서 노출하지 않는다.
- `/pair` 요청에는 token address가 아니라 DEX pool address를 사용한다.
- `quote_token`은 내부 market metadata이며 더 이상 별도 관리 API가 아니다.

## Dev 반영 전 체크

1. API 변경과 갱신된 migration gitlink를 커밋한다.
2. migration 커밋 `9694aa7`이 대상 remote에서 접근 가능한지 확인한다.
3. `cargo fmt --all -- --check`, `cargo test`, `SQLX_OFFLINE=true cargo build --release`를 통과시킨다.
4. disposable PostgreSQL에 `0001_init.sql`을 적용해 retained query를 확인한다.
5. dev 전용 Redis/PostgreSQL/RPC/AWS 설정으로 배포한다. 서비스 실행은 Redis 상태를 변경할 수 있으므로 공유 Redis를 사용하지 않는다.

이 문서는 로컬 변경 상태를 설명한다. Canonical migration 커밋과 로컬 gitlink 갱신은 완료했지만 API 커밋, 푸시 또는 dev 배포 완료를 의미하지 않는다.
