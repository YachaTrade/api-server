# feat/cms-whitelist-token

## Purpose

admin이 `whitelist_token`(Select Token 모달 4-tier 화이트리스트)을 CMS로 추가/수정/숨김/조회. 마이그레이션 하드코딩 대신 런타임 등록 — WMON/USDC/USDT 등 주소를 마이그레이션 없이 화이트리스트에 넣기. base: `v2`.

## Changes

- 설계/플랜: `docs/superpowers/specs/2026-06-02-cms-whitelist-token-design.md`, `docs/superpowers/plans/2026-06-02-cms-whitelist-token.md`. API 변경: `docs/V2_API_CHANGES.md` (CMS 섹션)
- `POST /cms/whitelist-token` (admin) — upsert `{token_id, sort_order, enabled?}`. 소프트삭제 = `enabled:false`
- `GET /cms/whitelist-token` (admin) — 전체 목록(disabled 포함), sort_order ASC, symbol/name/image는 token/dex_token/quote_token LEFT JOIN
- token_id 검증 = `valid_account_id`(순수 체크섬). external 토큰은 베니티 접미사 없어 `valid_token_id` 쓰면 거부됨 (메모리 `reference_valid_token_id_vanity`)
- 인가: `authenticate_user` 세션 + admin 인가는 **write pool**(`verify_admin_on_writer`, replica-lag 우회). upsert는 `INSERT ... SELECT WHERE EXISTS(admin) ON CONFLICT DO UPDATE` 원자 가드 → 비-admin은 신규 삽입/기존 행 덮어쓰기 모두 불가
- 메타 존재 검증: token/dex_token/quote_token 중 어디에도 없으면 404 (모달 빈 항목 방지)
- 스키마 변경 없음 (`whitelist_token` 테이블 기존재, Naddotfun/migrations#49)
- 변경: `types/cms/mod.rs`, `controllers/cms/mod.rs`, `services/cms/mod.rs`, `router/cms/{path,handler,mod}.rs`, `main.rs`(openapi)
- TDD: 컨트롤러 `#[sqlx::test]` 7케이스(삽입/충돌갱신/비-admin 미삽입/비-admin 기존행 미덮어쓰기/메타없음/quote_token경유/list 정렬+disabled+메타). `cargo test --lib cms` 21 passed

## Outcome

(PR/머지 시 작성)
