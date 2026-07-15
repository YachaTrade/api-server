# CMS Dev Post API 문서 형식 설계

## 목표와 범위

`docs/dev-post-api.md`의 CMS moderation 계약을 기존 Dev Post endpoint 문서와 같은 독립
섹션 형식으로 정리한다. 삭제와 복구를 각각 하나의 `### 번호. 제목 (METHOD /path)` 섹션으로
기록하고, 공통 운영 절차는 별도의 CMS 운영 전환 섹션에 둔다. 이 작업은 문서 구조와 표현만
정의하며 API 구현, 라우팅, 스키마, 상태 코드 계약의 변경은 범위 밖이다.

## 목표 외 사항

- endpoint 경로, HTTP method, 인증 방식, 상태 코드, 204 응답 의미를 재설계하지 않는다.
- title/body 보존, 관계 행 보존, pin 처리, audit와 commit outcome-unknown 의미를 바꾸지 않는다.
- 캐시 키·TTL 정책이나 migration SQL을 추가하지 않는다.
- 기존 일반 author 삭제 섹션을 CMS 계약으로 합치거나 중복 설명하지 않는다.

## 문서 구조

CMS 공통 소개는 관리자 인증(EIP-55 canonical 주소), author endpoint와의 구분, 공통 동작
불변조건을 설명한다. 그 뒤 다음 두 독립 섹션을 같은 순서로 둔다.

1. `### N. CMS 게시물 삭제 (`DELETE /cms/dev-post/{post_id}`)`
2. `### N+1. CMS 게시물 복구 (`POST /cms/dev-post/{post_id}/restore`)`

각 섹션은 기존 API 섹션의 양식을 정확히 따른다.

- 설명 및 동작 규칙
- `#### 요청`: Method, 인증
- `#### Path Parameters`: `post_id` 타입과 의미 표
- `#### 응답`: 성공 `204 No Content`, 빈 body 명시
- `#### 에러 응답`: endpoint별 상태 코드와 조건을 개별 bullet로 열거
- 필요한 경우 마지막에 idempotency, audit, content/relationship preservation 같은 동작 notes

삭제는 물리 post row 부재의 404와 pin mapping 조건부 제거를 명시한다. 이미 삭제된 post의
반복 DELETE는 audit를 남기는 멱등 no-op이며 빈 204다. 복구는 물리 post row 부재의 404와 복구에
필요한 token row 부재의 409를 구분한다. 이미 live/복구된 post의 RESTORE도 audit를 남기는 멱등
no-op이며 빈 204다. 두 섹션 모두 request body 없음과 관리자 세션 요구를 명시한다.

## 공통 CMS 운영 전환 섹션

endpoint 섹션 뒤 별도 `### CMS 운영 전환 및 관찰` 섹션을 둔다. 이 섹션은 모든 read/write와
직접 caller gate 및 pre-title node drain, audit readiness, row/sample·backup/WAL/free-space·
replica 검증, 네 cache TTL `<=60_000ms`, migration `0041` 적용과 **point of no return**,
title/disposable fixture 및 replica convergence 검증을 포함한다.

cache cleanup은 `REDIS_KEY_PREFIX`가 적용된 다음 exact-key 삭제로 제한한다. 알려진 삭제 집합은
global feed의 `devpost:feed:global`, `devpost:feed:v2:global`, `devpost:feed:v3:global`, 해당 token
feed의 `devpost:feed:{token_id}`, `devpost:feed:v2:{token_id}`, `devpost:feed:v3:{token_id}`,
detail의 `devpost:detail:{post_id}`, `devpost:detail:v2:{post_id}`, trending의
`devpost:trending`, `devpost:trending:v2`다. 이 10개 exact key 삭제를 모두 완료한 뒤에만
`devpost:ranking:generation`을 증가시키고 payload는
`devpost:ranking:v2:{generation}:{page}:{limit}`을 사용한다. 알려지지 않은 key만 자연 만료시키며
열거하거나 wildcard 삭제하지 않는다. 인증 smoke test 이후 CMS controls를 활성화한다. migration
commit 전에는 abort/rollback과 구 binary 복구가 가능하지만, 이후 pre-title binary 실행·body
재결합·title 제거·compatibility response 복구는 금지하고 title-aware corrective binary로
roll-forward한다. CMS controls만 끄는 것은 audit/content가 authoritative인 동안 안전하다고
명시한다. outcome-unknown, reconciliation, lock/deadlock, replica lag, cache PTTL/late-fill,
old-schema cache access, rendering 및 pgactive/conditional pin 경계를 모니터링한다.

## 기존 계약 보존 및 검증

문서 개편 전후 endpoint 경로와 method, 인증, 상태 코드, 204 빈 body, 보존/감사 semantics가
동일해야 한다. 기존 일반 Dev Post 섹션의 번호·내용은 변경하지 않는다. 검증은 다음을 수행한다.

- 삭제된 standalone 문서 경로에 대한 active 링크/include 참조가 없다.
- 두 CMS 섹션 모두 요청·path parameter·응답·에러 하위 heading을 갖는다.
- `point of no return`, `0041`, TTL 상한, global/token feed 6개·detail 2개·trending 2개의 exact
  삭제 key, 10개 삭제가 모두 완료된 뒤에만 수행하는 ranking generation 증가, unknown-key 자연
  만료, rollback 금지 문구가 운영 섹션에 있으며 key별 문서 위치로 이 순서를 검증한다.
- CMS DELETE/RESTORE의 404는 물리 post row 부재만 뜻하며, RESTORE의 409는 token row 부재만
  뜻한다. 반복 DELETE와 이미 live인 RESTORE는 감사되는 빈 204 no-op으로 검증한다.
- 문서 diff가 CMS 섹션과 필요한 참조만 포함하며 `git diff --check`가 통과한다.

## 자체 리뷰

- 미완성 문구 없음.
- 삭제와 복구의 409/404 조건 및 반복 요청의 204 no-op 조건을 서로 혼동하지 않음.
- endpoint별 계약과 공통 운영 전환 책임이 분리되어 구현 범위가 확장되지 않음.
- 기존 API 계약 보존과 standalone 문서 제거 요구가 일관됨.
