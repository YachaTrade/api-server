# feat/cms-creator-fee

## Purpose

CMS(admin)용 읽기 전용 엔드포인트 `GET /cms/analytics/creator-fee` 추가 — 토큰 + 기간을 받아
총 거래량 / 순수 creator fee / sell fee / 총 creator fee 를 집계해 반환.
기존 수동 CTE 콘솔 쿼리를 API로 노출. base: `v2`.

## Changes

- 설계 문서: `docs/superpowers/specs/2026-06-01-cms-creator-fee-design.md`
- `GET /cms/analytics/creator-fee` (admin) 추가 — token_id + from/(to) 받아 total_volume / pure_creator_fee / sell_fee / total_creator_fee 반환
- `token.version`으로 v1/v2 자동 분기: v1=lp_collect_history + fee_distribute_history, v2=v2_creator_fee_distribution(DISTRIBUTE). total_volume은 공통 swap 집계
- token 주소 핸들러 진입 시 `valid_token_id`로 EIP-55 체크섬 정규화 후 사용. 미존재 토큰 404
- multi-quote 정합성(codex P2): 금액을 `10^18` 고정이 아니라 `market→quote_token.decimals`로 스케일. 응답에 `quote_id`/`quote_symbol`/`decimals` 추가. USDC(6) 등 non-18 quote 정확 처리
- TDD: 컨트롤러 `creator_fee_stats` 7 케이스(v1/v2/기간경계/to생략/zero/missing/quote-decimals) `#[sqlx::test]`
- 변경: `types/cms/analytics.rs`, `controllers/cms/analytics.rs`, `router/cms/analytics/{path,mod,handler}.rs`, `main.rs`(openapi)

## Review

- codex review (base v2): GATE PASS, P2 1건(non-18 quote decimals) → fix 적용 + 회귀 테스트 추가

## Outcome

(PR/머지 시 작성)
