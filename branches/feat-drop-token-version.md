# feat/drop-token-version

## Purpose

GIWA는 단일 버전 배포라 DB에 `token.version` 컬럼이 없다. api-server가 이 컬럼을 읽는 모든 경로를 제거한다.

## Changes

- `TokenInfo.version` 응답 필드 삭제, row struct 15개의 `version: TokenVersion` 필드 + import 삭제, 전파 19곳 삭제
- SQL 프로젝션 `t.version` 25곳 삭제 (`cms/analytics.rs`의 String 디코딩 포함, `dividend`의 공용 `TOKEN_MARKET_COLUMNS` 포함)
- 동작 수렴 2건: `token/create.rs`·`token/gift_fee.rs`의 `reward_info` V1/V2 분기 → V2 로직 단일화
- `dex/tokens.rs`: `WHERE t.version='V2'` 필터 4곳 삭제 (GIWA에선 `token` 테이블 전체가 nadfun 토큰). CTE명 `v2`→`nadfun`, wire 값 `nadfun_v2`는 유지, 생성 불가능해진 `nadfun_v1` enum 값은 제거
- 테스트 INSERT의 version 컬럼 제거
- 유지: `TokenVersion` enum 자체 — CREATE2 salt 마이닝(`types/token/salt.rs`, `services/token/salt.rs`)과 x_verification의 본딩커브/impl 주소 쌍 선택용 **요청 파라미터**이며 DB 읽기가 아님
- 검증: build + lib 321 passed / 4 failed — 실패 4건은 dev_post 하니스 문제로 베이스라인 동일 실패(무관). 실행 Codex(gpt-5.6-sol medium), 검증·커밋 orchestrator

## Outcome

- (머지 시 작성)
