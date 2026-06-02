# feat/dex-token-listing

## Purpose

Swap "Select Token" 모달용 토큰 리스팅/검색을 `GET /dex/tokens` 하나로 통합 (Notion "Dex Token Listing" 스펙). 4-tier 우선순위(보유/미보유 × 화이트리스트/nadfun V2), prefix 검색, external 토큰 구분, `GET /dex/search` deprecate. base: `v2`.

## Changes

- 설계/플랜: `docs/superpowers/specs/2026-06-02-dex-token-listing-design.md`, `docs/superpowers/plans/2026-06-02-dex-token-listing.md`. API 변경: `docs/V2_API_CHANGES.md` (기존→변경 before/after 포함)
- 신규 `whitelist_token` 테이블 (token_id, sort_order, enabled). migrations 서브모듈 PR Naddotfun/migrations#49 (@c8325ba, v2 머지). seed: MON, LVMON. **WMON/USDC/USDT 주소 미확정 → 후속 seed**
- `GET /dex/tokens`: `q` 파라미터 추가 → q 없으면 4-tier 기본 리스트(화이트리스트+nadfun V2만, external/V1 제외), q 있으면 검색. 정렬: 보유 티어 balance_usd desc, 미보유 화이트리스트 고정순서, 미보유 V2 마켓캡 desc. enriched CTE + outer ORDER BY로 tier 정렬(인증 유저 tier-3 sort_order NULL 버그 수정)
- 검색: case-insensitive **prefix**(기존 substring→변경). 텍스트→symbol/name prefix(external 제외), full CA(42자)→exact 매칭(external 노출), partial `0x`→token_id prefix(external 제외). full CA는 핸들러+위임 경로에서 `normalize_ca_query`로 체크섬 정규화
- `DexTokenEntry` 확장: `token_type`/`is_external`/`is_held`/`balance_usd`/`tier` 신규 필드
- `GET /dex/search`: `#[deprecated]` + `TokensController::list_tokens` 위임(세션 주소=account). 한 릴리스 유지 후 제거
- 신규 `crate::utils::normalize_ca_query` 헬퍼 (full-CA → EIP-55 체크섬, 아니면 passthrough)
- 변경: `types/dex/{tokens,search}.rs`, `controllers/dex/{tokens,search}.rs`, `router/dex/{tokens,search,mod}.rs`, `services/dex/mod.rs`, `utils/mod.rs`, `migrations(-test)/0031` + `v2_upgrade_new_tables.sql`
- TDD: `cargo test --lib` 89 passed (dex tokens 14 + search 위임 2 + 회귀/검색 케이스 포함). codex review [P1](프로덕션 마이그레이션 누락) → 서브모듈 머지 + gitlink bump로 해소

## Outcome

- PR: Naddotfun/api-server#119 (base `v2`)
- migrations: Naddotfun/migrations#49 머지(@c8325ba), 부모 gitlink 반영
- 남은 follow-up: WMON/USDC/USDT 화이트리스트 주소 seed, FE `/dex/search` → `/dex/tokens?q=` 컷오버 후 deprecated 제거, 스테이징 실측(wallet connected/not)
