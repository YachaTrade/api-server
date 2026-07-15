# CMS Dev Post API Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reformat the CMS moderation documentation in `docs/dev-post-api.md` into two independently readable endpoint sections plus one shared operations section without changing the API contract.

**Architecture:** Keep the existing CMS moderation heading and contract facts, but split DELETE and RESTORE into numbered `###` endpoint sections. Move deployment, cache, rollback, and monitoring guidance into a dedicated CMS operations section; no source or API implementation changes.

**Tech Stack:** Markdown, repository documentation-contract test in Rust (`src/main.rs`), `rg`, Git.

## Global Constraints

- Preserve endpoint paths, methods, authentication, status codes, empty 204 bodies, and moderation semantics exactly.
- Do not modify API implementation, migrations, general author-delete documentation, or standalone deleted-document paths.
- Keep all four cache families and TTL bound `<=60_000ms`, migration `0041`, and the exact phrase `point of no return`.

---

### Task 1: Reformat CMS moderation documentation

**Files:**
- Modify: `docs/dev-post-api.md` CMS moderation section (currently after author pin sections)
- Verification only: existing `src/main.rs` `documentation_contract` test, compiled in the package binary `api-server` (no source edit)

**Interfaces:**
- Consumes: `docs/superpowers/specs/2026-07-15-cms-dev-post-api-format-design.md` and the existing CMS endpoint facts in `docs/dev-post-api.md`.
- Produces: two independent CMS endpoint sections and one shared CMS operations section in the canonical API document.

- [ ] **Step 1: Capture the pre-edit contract and references**

Run:

```bash
git show HEAD:docs/dev-post-api.md | sed -n '400,450p'
rg -n 'cms-dev-post-moderation\.md' docs/dev-post-api.md docs/V2_API_CHANGES.md src || true
```

Expected: existing CMS facts are visible; the canonical active docs/source paths contain no reference to the deleted standalone document.

- [ ] **Step 2: Rewrite the CMS section using the established endpoint shape**

In `docs/dev-post-api.md`, retain the common CMS authentication and invariants, then add these exact structural elements:

```markdown
### [next number]. CMS 게시물 삭제 (`DELETE /cms/dev-post/{post_id}`)
#### 요청
#### Path Parameters
#### 응답
#### 에러 응답

### [next number]. CMS 게시물 복구 (`POST /cms/dev-post/{post_id}/restore`)
#### 요청
#### Path Parameters
#### 응답
#### 에러 응답

### CMS 운영 전환 및 관찰
```

The delete section must state no request body, admin session/EIP-55 authentication, `post_id` as BIGINT string, 204 with empty body, and exact conditions: 400 malformed id, 401 missing session, 403 non-admin, 404 only when the physical post row is missing, 500 internal error. An already-deleted physical post is an audited idempotent no-op (`changed=false`) returning empty 204. The restore section must state the same request/path/response shape and exact conditions: 400 malformed id, 401 missing session, 403 non-admin, 404 only when the physical post row is missing, 409 only when the token row required for restore is unavailable, 500 internal error. An already-live/already-restored post is an audited idempotent no-op (`changed=false`) returning empty 204. Preserve notes that content and relationship rows survive, delete conditionally removes pin mapping, audit writes are idempotent, and outcome-unknown requires reconciliation/retry.

The operations section must include: gating all reads/writes/direct callers and draining pre-title nodes; audit readiness; row count, disposable/representative samples without production inserts, backup/WAL/free-space, replica health; all four TTLs `<=60_000ms`; migration `0041` and **point of no return**; title/disposable fixture and replica convergence checks. Cache cleanup applies `REDIS_KEY_PREFIX` and deletes only this exact established set: global feed `devpost:feed:global`, `devpost:feed:v2:global`, `devpost:feed:v3:global`; token feed `devpost:feed:{token_id}`, `devpost:feed:v2:{token_id}`, `devpost:feed:v3:{token_id}`; detail `devpost:detail:{post_id}`, `devpost:detail:v2:{post_id}`; trending `devpost:trending`, `devpost:trending:v2`. It must delete all ten exact keys first; only after all ten deletions complete may it increment `devpost:ranking:generation`. Ranking payloads use `devpost:ranking:v2:{generation}:{page}:{limit}`. Legacy/v2 keys are not fallback sources. Unknown keys alone expire naturally and must not be enumerated or wildcard-deleted. Include authenticated smoke tests; pre-commit abort/rollback; post-commit prohibition on pre-title binaries, body rejoin, title removal, compatibility responses; title-aware roll-forward; CMS-only disable safety; pgactive/conditional pin boundaries; and monitoring signals.

- [ ] **Step 3: Run lightweight documentation validation**

Run:

```bash
git diff --check
rg -n 'cms-dev-post-moderation\.md' docs/dev-post-api.md docs/V2_API_CHANGES.md src || true
rg -n 'devpost:feed:global|devpost:feed:v2:global|devpost:feed:v3:global|devpost:feed:\{token_id\}|devpost:feed:v2:\{token_id\}|devpost:feed:v3:\{token_id\}|devpost:detail:\{post_id\}|devpost:detail:v2:\{post_id\}|devpost:trending|devpost:trending:v2|devpost:ranking:generation|devpost:ranking:v2:\{generation\}:\{page\}:\{limit\}' docs/dev-post-api.md
operations=$(sed -n '/^### CMS 운영 전환 및 관찰/,/^---$/p' docs/dev-post-api.md)
generation_line=$(printf '%s\n' "$operations" | rg -n -F '위 10개 exact key 삭제를 모두 완료한 뒤에만 `devpost:ranking:generation`' | cut -d: -f1)
for key in '`devpost:feed:global`' '`devpost:feed:v2:global`' '`devpost:feed:v3:global`' \
  '`devpost:feed:{token_id}`' '`devpost:feed:v2:{token_id}`' '`devpost:feed:v3:{token_id}`' \
  '`devpost:detail:{post_id}`' '`devpost:detail:v2:{post_id}`' '`devpost:trending`' \
  '`devpost:trending:v2`'; do
  key_line=$(printf '%s\n' "$operations" | rg -n -F "$key" | head -1 | cut -d: -f1)
  test -n "$key_line" && test "$key_line" -lt "$generation_line"
done
rg -n '물리 `dev_post` row가 없음|이미 삭제된 상태도|이미 live/복구된 상태도|token row를 사용할 수 없음|멱등 no-op|빈 `204`' docs/dev-post-api.md
rg -n 'point of no return|0041|<=60_000ms|outcome-unknown|pgactive' docs/dev-post-api.md
```

Expected: diff-check exits 0; no deleted-file reference is printed from the active canonical docs and source paths listed above; both endpoints distinguish physical-row 404 from restore-token 409 and document audited empty-204 no-ops; all ten exact deletion keys are present and every key line precedes the explicit only-after-all-deletions ranking generation increment; unknown-key natural expiry and required operations terms are present. Historical specs/plans are intentionally excluded from this active-reference check.

- [ ] **Step 4: Run the existing contract test**

Run:

```bash
cargo test --bin api-server documentation_contract
```

Expected: the `api-server` binary test passes. If compilation fails with `No space left on device`, record that exact static-validation limitation and do not retry or broaden the change.

- [ ] **Step 5: Commit the documentation-only change**

```bash
git add docs/dev-post-api.md
git commit -m "docs: format CMS dev-post API endpoints"
```

Expected: commit contains only the canonical API documentation edit.

## Plan Self-Review

- Spec coverage: scope/non-goals, endpoint structure, contract preservation, operations runbook, validation, and no-standalone-reference checks are mapped to Task 1.
- Completeness scan: no unfinished or unspecified implementation step appears in the plan.
- Type/structure consistency: both endpoints use the same request/path/response/error headings, retain their distinct error status-code sets, and document repeated/already-live requests as audited empty-204 no-ops.
