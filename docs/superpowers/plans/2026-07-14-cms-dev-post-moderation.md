# CMS Dev Post Moderation Implementation Plan

> [!CAUTION]
> **SUPERSEDED — DO NOT EXECUTE.** This moderation-only plan predates explicit
> `title`/description-only `body`. Use
> [`2026-07-14-cms-dev-post-moderation-and-title.md`](2026-07-14-cms-dev-post-moderation-and-title.md)
> for all migration, implementation, test, rollout, rollback, review, and PR work.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add audited, idempotent CMS Dev Post delete/restore endpoints while preserving author-delete behavior, unpinning atomically, and invalidating every public Dev Post cache representation safely.

**Architecture:** `DevPostController` owns primary-Postgres authorization, lock order, state transitions, audit insertion, rollback, and commit-outcome capture; `DevPostService` owns audit-ID reconciliation, structured logs, and best-effort cache invalidation. Redis ranking moves from enumerable legacy keys to a captured generation namespace, and the CMS router stays a thin authenticated HTTP adapter.

**Tech Stack:** Rust 2024, Axum 0.7, sqlx 0.8/PostgreSQL, Redis 0.29, UUID v4, tracing, utoipa/OpenAPI, `#[sqlx::test]`, pgactive production writers.

**Approved spec:** `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md`

## Global Constraints

- API integration base and PR base are `v2`; the design-time base is `2c778a9`, but execution starts by fetching the latest `origin/v2`.
- `migrations/` is a separate repository/submodule. Its feature branch and PR also target `v2`; the API gitlink must point to the final migrations squash commit.
- Never merge a migrations PR without an explicit user approval after the PR URL and verification result are reported. After approval, run `gh pr ready`, verify `isDraft=false`, then merge with squash and never pass `--delete-branch`.
- After every merge, run `git fetch origin v2:v2` and verify local `v2` equals `origin/v2`.
- EVM addresses are canonical EIP-55 strings. Compare with exact `=` only; never introduce `LOWER()`, case folding, or case-insensitive indexes.
- `dev_post_moderation_log` uses application-generated UUID v4 IDs, two CHECK constraints, two indexes, and deliberately has no foreign keys or backfill.
- The pre-merge migration contract must run with `DATABASE_URL` overridden from an explicit disposable non-production `DATABASE_TEST_URL`; `#[sqlx::test]` creates its temporary databases there. Never qualify the Draft migration against a production PostgreSQL server.
- CMS DELETE and RESTORE accept no body. Existing posts are idempotent and return a truly empty `204`; malformed/zero/negative IDs are `400`, unauthenticated is `401`, non-admin is `403`, absent physical post is `404`, and orphan restore is `409`.
- Authentication middleware runs before handler path extraction. Admin membership is locked on the write pool before any `dev_post` lookup.
- Same-primary lock order is CMS `admin -> token -> post`; author/pin paths remain `token -> post`. CMS uses token `FOR KEY SHARE` then post `FOR UPDATE`; existing author/pin token-lock strength is not weakened.
- Author delete remains author-only and non-idempotent: wrong author `403`, missing/already-deleted `404`, no moderation audit row.
- A real restore clears only `deleted_at` and any stale historical pin. A no-op restore of an already-live post preserves its current pin. Moderation never edits images, likes, polls, options, votes, body, author, or timestamps other than `deleted_at` and audit `created_at`.
- Pre-commit failures explicitly roll back best-effort and do not invalidate caches. A `COMMIT` error is outcome-unknown; CMS reconciles by exact audit UUID on the write pool, while author delete returns `500 outcome_unknown` without an audit lookup.
- Full post invalidation independently attempts global/token feed legacy+v2 keys, detail, trending, and ranking generation. Redis failures never change a confirmed database result.
- New ranking code never reads or writes `devpost:ranking:{page}:{limit}`. It captures `devpost:ranking:generation` once and uses `devpost:ranking:v2:{generation}:{page}:{limit}` for both GET and SET.
- A missing generation means `0`; a generation read error bypasses ranking cache entirely. Moderation bumps the counter with atomic `INCR`.
- Do not use Redis `KEYS`, `SCAN`, wildcard deletion, prefix-wide deletion, or `FLUSHALL` in this feature or its tests. Every Redis-touching Cargo process must use a dedicated disposable non-production `REDIS_TEST_URL` and a fresh, nonempty `REDIS_KEY_PREFIX` formed as `test-cms-dev-post-moderation-` plus a generated UUID, supplied before process start. Tests assert that namespace before touching Redis, and every exact global/token/detail/trending/generation/ranking key is built through the configured prefix. An in-process mutex is only supplemental serialization; it is never the isolation boundary and cannot protect separately launched router/test processes.
- Same-primary tests may assert serialization. pgactive cross-writer tests must not assert a global winner, exactly one `changed=true`, global audit ordering, or last-HTTP-request-wins.
- CMS controls stay disabled until all old API nodes drain, replica lag is healthy, the exactly prefixed trending key is deleted, and the exactly prefixed ranking generation key is incremented.
- Use `apply_patch` for file contents, preserve unrelated work, stage only files listed by the current task, and end every implementation task with focused tests, `cargo fmt --all -- --check`, and a scoped commit.

---

## File Responsibility Map

- `migrations/0040_dev_post_moderation_log.sql` (migrations repository) — permanent audit table, checks, and lookup indexes; no FK and no backfill.
- `migrations-test/0040_dev_post_moderation_log.sql` — symlink to the merged production migration.
- `tests/dev_post_moderation_migration.rs` — schema contract, constraints, indexes, no-FK retention, and no-backfill verification.
- `src/controllers/dev_post/moderation.rs` — shared delete primitive, CMS admin-first transaction policy, restore policy, pending/commit-unknown result types, audit insert, and write-pool reconciliation lookup.
- `src/controllers/dev_post/moderation_tests.rs` — authorization, idempotency, rollback, relationship preservation, lock/concurrency, and optional pgactive qualification tests.
- `src/controllers/dev_post/mod.rs` — register/re-export the focused moderation module and remove the old inline author-delete SQL.
- `src/controllers/dev_post/pin.rs` — existing token-before-post lock helper consumed by author delete; pin authorization and lock strength stay unchanged.
- `src/db/redis/mod.rs` — exact ranking generation/v2 key helpers, captured-generation get/set, atomic bump, and exact trending delete.
- `src/services/dev_post/mod.rs` — ranking cache algorithm, author/CMS mutation orchestration, commit reconciliation, full cache invalidation, and operational logs.
- `src/services/dev_post/moderation_tests.rs` — service-level reconciliation, cache failure isolation, no-op invalidation, ranking bypass, and legacy-ignore tests.
- `src/router/cms/path.rs` — distinct Axum runtime and OpenAPI path strings for delete/restore.
- `src/router/cms/handler.rs` — thin `Path<i64>` handlers returning `AppResult<StatusCode>`.
- `src/router/cms/mod.rs` — protected route registration and real-router status/body/auth tests.
- `src/main.rs` — utoipa path registration and OpenAPI/document contract tests.
- `docs/cms-dev-post-moderation.md` — operator/API contract, audit semantics, rollout, rollback, and pgactive boundary.
- `docs/dev-post-api.md` — author-delete versus CMS moderation and corrected trending/ranking caching behavior.
- `docs/V2_API_CHANGES.md` — two CMS endpoints while keeping the Dev Post endpoint count at 13.

### Task 1: Prepare the isolated execution branch and prove the baseline

**Files:**
- Read: `AGENTS.md` instructions supplied for this repository
- Read: `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md`
- Read: `docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md`

**Interfaces:**
- Consumes: approved design branch based on `2c778a9` and the already-created isolated worktree.
- Produces: clean `feat/cms-dev-post-moderation` branch rebased onto the latest local `v2`, initialized migrations submodule, and a recorded green baseline.

- [ ] **Step 1: Load the execution skills and inspect the worktree**

Use `superpowers:using-git-worktrees`, then `superpowers:subagent-driven-development`. Run:

```bash
pwd
git status --short
git branch --show-current
git worktree list
```

Expected: current path is the isolated moderation worktree; only the approved spec/plan commits are present and the worktree is clean.

- [ ] **Step 2: Fetch the integration base**

```bash
git fetch origin v2:v2
```

Expected: local `v2` is updated from `origin/v2`.

- [ ] **Step 3: Create and rebase the feature branch**

```bash
git switch -c feat/cms-dev-post-moderation
git rebase --onto v2 2c778a9
git merge-base --is-ancestor v2 HEAD
```

Expected: all commands exit `0`; the final command proves the feature branch contains the latest local `v2` plus the approved documentation commits.

- [ ] **Step 4: Initialize the submodule at the API gitlink**

```bash
git submodule update --init migrations
git submodule status migrations
```

Expected: `migrations` is initialized at the commit recorded by the latest API `v2`; the status line does not begin with `-`.

- [ ] **Step 5: Verify baseline formatting**

```bash
set -a
source .env
set +a
cargo fmt --all -- --check
```

Expected: exit `0`.

- [ ] **Step 6: Verify the baseline compiles**

```bash
cargo check --all-targets --all-features
```

Expected: exit `0`.

- [ ] **Step 7: Run the untouched baseline tests**

```bash
test -n "${REDIS_TEST_URL:-}" || {
  echo "REDIS_TEST_URL must point to a dedicated disposable non-production Redis" >&2
  exit 1
}
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test --all-targets --all-features
```

Expected: every existing test passes with `0 failed`; the baseline never connects to a shared production Redis and has a process-unique nonempty key namespace.

- [ ] **Step 8: Run baseline Clippy**

```bash
cargo clippy --all-targets --all-features
```

Expected: exit `0`. If any baseline gate is not green, report the exact pre-existing failure before changing code.

- [ ] **Step 9: Record the baseline without making a commit**

```bash
git status --short
git log --oneline -3
```

Expected: clean status; no production implementation commit exists yet.

### Task 2: Create the permanent audit migration in the migrations repository

**Files:**
- Create in migrations worktree: `0040_dev_post_moderation_log.sql`

**Interfaces:**
- Consumes: latest migrations `v2`, whose design-time last numbered migration is `0039_dev_post_pin.sql`.
- Produces: additive `dev_post_moderation_log` schema on a separate migrations PR; no API gitlink change until that PR is explicitly approved and merged.

- [ ] **Step 1: Recheck the latest migrations number**

```bash
git -C migrations fetch origin v2:v2
CANONICAL_LAST="$(
  git -C migrations ls-tree --name-only v2 \
    | rg '^00[0-9]{2}_[a-z0-9][a-z0-9_]*\.sql$' \
    | LC_ALL=C sort \
    | tail -1
)"
test -n "$CANONICAL_LAST"
test "$CANONICAL_LAST" != "1000_delete.sql"
printf '%s\n' "$CANONICAL_LAST"
LAST_NUMBER="${CANONICAL_LAST%%_*}"
NEXT_NUMBER="$(printf '%04d' "$((10#$LAST_NUMBER + 1))")"
printf '%s\n' "${NEXT_NUMBER}_dev_post_moderation_log.sql"
```

Expected: the last canonical four-digit migration in the `0000`–`0099` range is `0039_dev_post_pin.sql`, so the candidate is `0040_dev_post_moderation_log.sql`. `1000_delete.sql` and every other noncanonical filename are deliberately excluded from sequencing. If `CANONICAL_LAST` is not `0039_dev_post_pin.sql`, stop before creating the migration and use `apply_patch` to replace every `0040_dev_post_moderation_log.sql` filename reference with `${NEXT_NUMBER}_dev_post_moderation_log.sql` across both the approved spec and this entire plan, including the file map, Tasks 2–3, Task 16, and the final checklist. The checksum test address ending in `4040` is not a filename and must not be changed.

After a renumber, prove the replacement and commit the documentation-only decision before continuing:

```bash
CANONICAL_LAST="$(
  git -C migrations ls-tree --name-only v2 \
    | rg '^00[0-9]{2}_[a-z0-9][a-z0-9_]*\.sql$' \
    | LC_ALL=C sort \
    | tail -1
)"
LAST_NUMBER="${CANONICAL_LAST%%_*}"
NEXT_NUMBER="$(printf '%04d' "$((10#$LAST_NUMBER + 1))")"
OLD_MIGRATION="$(printf '%s%s' '0040_dev_post_' 'moderation_log.sql')"
NEW_MIGRATION="${NEXT_NUMBER}_dev_post_moderation_log.sql"
test -z "$(rg -n --fixed-strings "$OLD_MIGRATION" \
  docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md || true)"
rg -n --fixed-strings "$NEW_MIGRATION" \
  docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md
git add \
  docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md
git diff --cached --check
git commit -m "docs: renumber cms moderation migration to ${NEXT_NUMBER}"
```

Expected when renumbering is required: no old filename remains, every spec/plan reference uses the new canonical number, and the API feature branch has an explicit docs/plan commit. When `CANONICAL_LAST` is `0039`, skip this conditional commit.

- [ ] **Step 2: Create the separate migrations worktree**

```bash
git -C migrations worktree add /private/tmp/migrations-cms-dev-post-moderation -b feat/cms-dev-post-moderation v2
```

Expected: the new worktree is on `feat/cms-dev-post-moderation` based on migrations `v2`.

- [ ] **Step 3: Add the exact migration**

Use `apply_patch` in the migrations worktree to create:

```sql
CREATE TABLE IF NOT EXISTS dev_post_moderation_log (
    id               UUID        PRIMARY KEY,
    post_id          BIGINT      NOT NULL CHECK (post_id > 0),
    token_id         VARCHAR(42) NOT NULL,
    admin_account_id VARCHAR(42) NOT NULL,
    action           VARCHAR(8)  NOT NULL
        CHECK (action IN ('DELETE', 'RESTORE')),
    changed          BOOLEAN     NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX IF NOT EXISTS idx_dev_post_moderation_log_post_created
    ON dev_post_moderation_log (post_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_dev_post_moderation_log_admin_created
    ON dev_post_moderation_log (admin_account_id, created_at DESC);
```

- [ ] **Step 4: Statistically verify the safety contract**

```bash
cd /private/tmp/migrations-cms-dev-post-moderation
rg -n "CREATE TABLE|CHECK|CREATE INDEX|REFERENCES|FOREIGN KEY|LOWER" 0040_dev_post_moderation_log.sql
git diff --check
```

Expected: one table, two CHECK clauses, two indexes; no `REFERENCES`, `FOREIGN KEY`, or `LOWER`; `git diff --check` is silent.

- [ ] **Step 5: Commit only the migrations repository change**

```bash
git add 0040_dev_post_moderation_log.sql
git commit -m "feat(dev-post): add moderation audit log"
```

Expected: one commit containing only `0040_dev_post_moderation_log.sql`.

- [ ] **Step 6: Push the migrations feature branch**

```bash
git push -u origin feat/cms-dev-post-moderation
```

Expected: push succeeds and sets the upstream.

- [ ] **Step 7: Open the Draft migrations PR**

```bash
gh pr create \
  --base v2 \
  --head feat/cms-dev-post-moderation \
  --draft \
  --title "feat(dev-post): add moderation audit log" \
  --body "Adds the UUID-keyed permanent Dev Post moderation audit table, action/post checks, and post/admin time indexes. No foreign keys and no data backfill."
gh pr view --json number,url,baseRefName,headRefName,isDraft
```

Expected: base `v2`, head `feat/cms-dev-post-moderation`, `isDraft=true`, and the PR URL is printed.

- [ ] **Step 8: Perform the initial migration review before executable qualification**

Use `superpowers:requesting-code-review` with this exact scope:

```text
Verify one additive table, UUID application key, two CHECK constraints, two indexes,
no FK/default UUID/backfill/LOWER(), base v2, and no branch deletion instruction.
```

Expected: record all findings, but do not request merge approval yet. Task 3 must first run the executable schema contract against the pushed Draft head, fix/test/commit/push/re-review until clean, and only then report the PR for explicit approval.

### Task 3: Prove the draft schema, obtain approval, publish it, and bump the gitlink

**Files:**
- Modify: `migrations` gitlink
- Create symlink: `migrations-test/0040_dev_post_moderation_log.sql -> ../migrations/0040_dev_post_moderation_log.sql`
- Create: `tests/dev_post_moderation_migration.rs`

**Interfaces:**
- Consumes: Draft migrations PR from Task 2; explicit user approval is requested only after the candidate schema has passed the executable contract and migration review.
- Produces: reviewed pre-merge schema evidence, then an API branch pinned to the final migrations squash commit with the same executable contract under `migrations-test`.

- [ ] **Step 1: Check out the exact pushed Draft migration commit in the API submodule without committing the gitlink**

From the API feature worktree:

```bash
MIGRATION_FEATURE_SHA="$(git -C /private/tmp/migrations-cms-dev-post-moderation rev-parse HEAD)"
test "$MIGRATION_FEATURE_SHA" = "$(git -C /private/tmp/migrations-cms-dev-post-moderation rev-parse origin/feat/cms-dev-post-moderation)"
git -C migrations fetch origin feat/cms-dev-post-moderation
git -C migrations checkout --detach "$MIGRATION_FEATURE_SHA"
test "$(git -C migrations rev-parse HEAD)" = "$MIGRATION_FEATURE_SHA"
test -f migrations/0040_dev_post_moderation_log.sql
```

Expected: the submodule worktree temporarily exposes the exact commit already pushed to the Draft PR. Nothing is staged or committed in the parent repository.

- [ ] **Step 2: Create and verify the final relative test symlink before running the contract**

```bash
ln -s ../migrations/0040_dev_post_moderation_log.sql migrations-test/0040_dev_post_moderation_log.sql
test "$(readlink migrations-test/0040_dev_post_moderation_log.sql)" = "../migrations/0040_dev_post_moderation_log.sql"
test -f migrations-test/0040_dev_post_moderation_log.sql
```

Expected: the candidate migration is available through the exact symlink that will be committed after merge.

- [ ] **Step 3: Write the executable migration contract**

Use `apply_patch` to create `tests/dev_post_moderation_migration.rs`:

```rust
use uuid::Uuid;

const ADMIN: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
const TOKEN: &str = "0x0000000000000000000000000000000000004040";

async fn seed_post(pool: &sqlx::PgPool) -> i64 {
    sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
        .bind(ADMIN)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'Moderation', 'MOD', 'img', $2, 0, '0x4040', 0)",
    )
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'audit me') RETURNING id",
    )
    .bind(TOKEN)
    .bind(ADMIN)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations-test")]
async fn moderation_log_schema_has_checks_indexes_no_fk_and_no_backfill(pool: sqlx::PgPool) {
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);

    let id_default: Option<String> = sqlx::query_scalar(
        "SELECT column_default FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'dev_post_moderation_log' AND column_name = 'id'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(id_default, None, "UUID must be application-generated");

    let foreign_keys: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.table_constraints \
         WHERE table_schema = 'public' AND table_name = 'dev_post_moderation_log' \
           AND constraint_type = 'FOREIGN KEY'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(foreign_keys, 0);

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' \
         AND tablename = 'dev_post_moderation_log' ORDER BY indexname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(indexes.iter().any(|name| name == "idx_dev_post_moderation_log_post_created"));
    assert!(indexes.iter().any(|name| name == "idx_dev_post_moderation_log_admin_created"));

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, 1, $2, $3, 'DELETE', true)",
    )
    .bind(id)
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(&pool)
    .await
    .unwrap();
    assert!(sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, 2, $2, $3, 'RESTORE', false)",
    )
    .bind(id)
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(&pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, 0, $2, $3, 'DELETE', false)",
    )
    .bind(Uuid::new_v4())
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(&pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, 1, $2, $3, 'EDIT', false)",
    )
    .bind(Uuid::new_v4())
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(&pool)
    .await
    .is_err());
}

#[sqlx::test(migrations = "./migrations-test")]
async fn moderation_audit_survives_admin_token_and_post_removal(pool: sqlx::PgPool) {
    let post_id = seed_post(&pool).await;
    let audit_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, $2, $3, $4, 'DELETE', true)",
    )
    .bind(audit_id)
    .bind(post_id)
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM admin WHERE account_id = $1")
        .bind(ADMIN)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM dev_post WHERE id = $1")
        .bind(post_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM token WHERE token_id = $1")
        .bind(TOKEN)
        .execute(&pool)
        .await
        .unwrap();

    let retained: (i64, String, String) = sqlx::query_as(
        "SELECT post_id, token_id, admin_account_id FROM dev_post_moderation_log WHERE id = $1",
    )
    .bind(audit_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained, (post_id, TOKEN.to_string(), ADMIN.to_string()));
}
```

- [ ] **Step 4: Run the executable schema contract against a temporary sqlx test database before approval**

```bash
set -a
source .env
set +a
test -n "${DATABASE_TEST_URL:-}" || {
  echo "DATABASE_TEST_URL must point to a disposable non-production PostgreSQL test server" >&2
  exit 1
}
DATABASE_URL="$DATABASE_TEST_URL" \
  cargo test --test dev_post_moderation_migration -- --nocapture
```

Expected GREEN before the migrations PR is marked ready, approved, or merged: `2 passed; 0 failed`. `#[sqlx::test]` creates a temporary test database, applies the candidate through `migrations-test`, and proves the live schema contract rather than relying on static SQL inspection.

- [ ] **Step 5: Restore the parent-recorded gitlink while leaving the reviewed test and symlink unstaged**

```bash
git submodule update --checkout migrations
RECORDED_MIGRATIONS_SHA="$(git ls-tree HEAD migrations | awk '{print $3}')"
test "$(git -C migrations rev-parse HEAD)" = "$RECORDED_MIGRATIONS_SHA"
test -z "$(git status --short migrations)"
git status --short migrations-test/0040_dev_post_moderation_log.sql tests/dev_post_moderation_migration.rs
```

Expected: the API gitlink is not modified before merge. Only the new unstaged contract file and exact relative symlink remain; the symlink may be dangling while the recorded pre-migration submodule commit is checked out.

- [ ] **Step 6: Run the migration fix/test/commit/push/re-review loop until clean**

First re-run `superpowers:requesting-code-review` against the exact pushed SHA, including the `2 passed; 0 failed` executable schema evidence. Then, for every migration or schema-test finding:

1. Apply the fix with `apply_patch` in `/private/tmp/migrations-cms-dev-post-moderation` or the API contract test.
2. Re-run the static checks from Task 2 Step 4.
3. Commit and push migration changes before testing the pushed commit:

   ```bash
   git -C /private/tmp/migrations-cms-dev-post-moderation add 0040_dev_post_moderation_log.sql
   git -C /private/tmp/migrations-cms-dev-post-moderation diff --cached --check
   git -C /private/tmp/migrations-cms-dev-post-moderation commit -m "fix(dev-post): address moderation migration review"
   git -C /private/tmp/migrations-cms-dev-post-moderation push
   MIGRATION_FEATURE_SHA="$(git -C /private/tmp/migrations-cms-dev-post-moderation rev-parse HEAD)"
   test "$MIGRATION_FEATURE_SHA" = "$(git -C /private/tmp/migrations-cms-dev-post-moderation rev-parse origin/feat/cms-dev-post-moderation)"
   git -C migrations fetch origin feat/cms-dev-post-moderation
   git -C migrations checkout --detach "$MIGRATION_FEATURE_SHA"
   test -f migrations-test/0040_dev_post_moderation_log.sql
   test -n "${DATABASE_TEST_URL:-}" || exit 1
   DATABASE_URL="$DATABASE_TEST_URL" \
     cargo test --test dev_post_moderation_migration -- --nocapture
   git submodule update --checkout migrations
   test -z "$(git status --short migrations)"
   ```

4. Re-run `superpowers:requesting-code-review` with the exact migration scope from Task 2 Step 8 plus the executable schema-test result.
5. Repeat until the reviewer reports no Critical or Important findings and the pushed PR head is the exact commit that passed the test.

If only the API schema test changes, use `apply_patch`, rerun the candidate checkout/test/restore sequence, and leave it unstaged for the final post-merge commit; do not create a migrations commit with no migration diff.

- [ ] **Step 7: Report the reviewed Draft PR and stop for explicit merge approval**

```bash
cd /private/tmp/migrations-cms-dev-post-moderation
MIGRATION_PR="$(gh pr view --json number --jq .number)"
REVIEWED_HEAD="$(git rev-parse HEAD)"
test "$REVIEWED_HEAD" = "$(gh pr view "$MIGRATION_PR" --json headRefOid --jq .headRefOid)"
git update-ref refs/codex/cms-dev-post-moderation-reviewed "$REVIEWED_HEAD"
gh pr view "$MIGRATION_PR" \
  --json number,url,baseRefName,headRefName,isDraft,state,headRefOid
```

Expected: the local `refs/codex/cms-dev-post-moderation-reviewed` ref freezes the exact reported PR head across the user-approval turn. Report the PR URL, that SHA, `2 passed; 0 failed`, and clean review result. Stop here. Do not call `gh pr ready` or merge until the user explicitly approves that exact reviewed head.

- [ ] **Step 8: After explicit approval, mark and squash-merge that exact migrations PR head in one shell**

```bash
cd /private/tmp/migrations-cms-dev-post-moderation
MIGRATION_PR="$(gh pr view --json number --jq .number)"
APPROVED_HEAD="$(git rev-parse refs/codex/cms-dev-post-moderation-reviewed)"
test "$APPROVED_HEAD" = "$(git rev-parse HEAD)"
test "$APPROVED_HEAD" = "$(gh pr view "$MIGRATION_PR" --json headRefOid --jq .headRefOid)"
gh pr ready "$MIGRATION_PR"
gh pr view "$MIGRATION_PR" --json number,url,isDraft,state,headRefOid
test "$(gh pr view "$MIGRATION_PR" --json isDraft --jq .isDraft)" = "false"
test "$(gh pr view "$MIGRATION_PR" --json headRefOid --jq .headRefOid)" = "$APPROVED_HEAD"
gh pr merge "$MIGRATION_PR" --squash
```

Expected: before merge, `isDraft=false`, `state=OPEN`, and `headRefOid` remains the reviewed head just approved by the user. The variables remain in the same shell through `gh pr merge`, so the exact approved head is squash-merged; no command contains `--delete-branch` or deletes a branch.

- [ ] **Step 9: Synchronize migrations `v2` immediately after merge**

```bash
git fetch origin v2:v2
test "$(git rev-parse v2)" = "$(git rev-parse origin/v2)"
git log --oneline -1 v2
```

Expected: local migrations `v2` exactly equals `origin/v2`, and the latest commit is the moderation audit squash commit.

- [ ] **Step 10: Point the API submodule at the final squash commit and rerun the identical contract**

From the API feature worktree:

```bash
cd /Users/gyu/project/nads-pump/api-server/.worktrees/cms-dev-post-moderation-design
git -C migrations fetch origin v2:v2
git -C migrations checkout --detach v2
test "$(git -C migrations rev-parse HEAD)" = "$(git -C migrations rev-parse origin/v2)"
test -f migrations-test/0040_dev_post_moderation_log.sql
test -n "${DATABASE_TEST_URL:-}" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" \
  cargo test --test dev_post_moderation_migration -- --nocapture
git submodule status migrations
```

Expected GREEN: the exact final relative symlink now resolves through the merged squash commit and the same two executable schema tests pass again.

- [ ] **Step 11: Commit the API gitlink, symlink, and schema test**

```bash
git add migrations migrations-test/0040_dev_post_moderation_log.sql tests/dev_post_moderation_migration.rs
git diff --cached --check
git commit -m "test(dev-post): wire moderation audit migration"
```

Expected: the commit contains one gitlink bump, one symlink, and one integration-test file.

### Task 4: Add Redis ranking-generation primitives and exact trending deletion

**Files:**
- Modify: `src/db/redis/mod.rs`

**Interfaces:**
- Consumes: existing `with_prefix`, `measure_redis!`, `RankingResponse`, and TTL constants.
- Produces:
  - `get_devpost_ranking_generation(&self) -> anyhow::Result<i64>`
  - `bump_devpost_ranking_generation(&self) -> anyhow::Result<i64>`
  - `get_devpost_ranking_response_for_generation(&self, generation: i64, page: i64, limit: i64) -> anyhow::Result<RankingResponse>`
  - `set_devpost_ranking_response_for_generation(&self, generation: i64, page: i64, limit: i64, response: &RankingResponse) -> anyhow::Result<()>`
  - `delete_devpost_trending(&self) -> anyhow::Result<()>`

- [ ] **Step 1: Add failing key/generation tests**

Extend the Redis test module with tests that use a UUID in every mutable test key:

```rust
fn assert_test_redis_namespace() {
    let raw = std::env::var("REDIS_KEY_PREFIX")
        .expect("Redis tests require an explicit REDIS_KEY_PREFIX");
    let uuid = raw
        .trim_end_matches(':')
        .strip_prefix("test-cms-dev-post-moderation-")
        .expect("refusing to touch Redis outside the CMS moderation test namespace");
    uuid::Uuid::parse_str(uuid).expect("REDIS_KEY_PREFIX must end in a per-process UUID");
    assert!(!crate::config::REDIS_KEY_PREFIX.is_empty());
    assert_eq!(
        crate::config::REDIS_KEY_PREFIX.as_str(),
        format!("{}:", raw.trim_end_matches(':'))
    );
}

#[test]
fn ranking_v2_key_contains_captured_generation_page_and_limit() {
    assert_test_redis_namespace();
    assert_eq!(
        devpost_ranking_v2_key(17, 3, 25),
        with_prefix("devpost:ranking:v2:17:3:25".to_string())
    );
    assert_ne!(
        devpost_ranking_v2_key(17, 3, 25),
        with_prefix("devpost:ranking:3:25".to_string())
    );
}

#[tokio::test]
async fn missing_generation_is_zero_and_concurrent_bumps_are_not_lost() {
    dotenv::dotenv().ok();
    assert_test_redis_namespace();
    let redis = RedisDatabase::new().await;
    let key = with_prefix(format!(
        "test:devpost:ranking:generation:{}",
        uuid::Uuid::new_v4().simple()
    ));
    assert_eq!(redis.read_generation_at(&key).await.unwrap(), 0);
    let (a, b, c) = tokio::join!(
        redis.bump_generation_at(&key),
        redis.bump_generation_at(&key),
        redis.bump_generation_at(&key)
    );
    let mut values = vec![a.unwrap(), b.unwrap(), c.unwrap()];
    values.sort_unstable();
    assert_eq!(values, vec![1, 2, 3]);
    let mut conn = redis.conn.as_ref().clone();
    conn.del::<_, ()>(&key).await.unwrap();
}

#[tokio::test]
async fn captured_generation_write_cannot_populate_the_next_generation() {
    dotenv::dotenv().ok();
    assert_test_redis_namespace();
    let redis = RedisDatabase::new().await;
    let unique_page = (uuid::Uuid::new_v4().as_u128() % 1_000_000) as i64 + 1000;
    let response = RankingResponse {
        rankings: Vec::new(),
        total_count: 7,
    };
    redis
        .set_devpost_ranking_response_for_generation(41, unique_page, 13, &response)
        .await
        .unwrap();
    assert_eq!(
        redis
            .get_devpost_ranking_response_for_generation(41, unique_page, 13)
            .await
            .unwrap()
            .total_count,
        7
    );
    assert!(
        redis
            .get_devpost_ranking_response_for_generation(42, unique_page, 13)
            .await
            .is_err()
    );
    let mut conn = redis.conn.as_ref().clone();
    conn.del::<_, ()>(devpost_ranking_v2_key(41, unique_page, 13))
        .await
        .unwrap();
}
```

- [ ] **Step 2: Run RED**

```bash
set -a
source .env
set +a
test -n "${REDIS_TEST_URL:-}" || {
  echo "REDIS_TEST_URL must point to a dedicated disposable non-production Redis" >&2
  exit 1
}
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test db::redis::devpost -- --nocapture
```

Expected: compile failure naming the missing generation/key helpers.

- [ ] **Step 3: Implement the exact helpers without removing legacy methods yet**

Add these private key/primitive helpers near the existing feed key helpers:

```rust
fn devpost_ranking_generation_key() -> String {
    with_prefix("devpost:ranking:generation".to_string())
}

fn devpost_ranking_v2_key(generation: i64, page: i64, limit: i64) -> String {
    with_prefix(format!(
        "devpost:ranking:v2:{generation}:{page}:{limit}"
    ))
}

fn devpost_trending_key() -> String {
    with_prefix("devpost:trending".to_string())
}
```

Add these methods to `impl RedisDatabase`:

```rust
async fn read_generation_at(&self, key: &str) -> Result<i64> {
    let mut conn = self.conn.as_ref().clone();
    let value: Option<i64> = measure_redis!(
        "redis.get_devpost_ranking_generation",
        conn.get::<_, Option<i64>>(key)
    )?;
    Ok(value.unwrap_or(0))
}

async fn bump_generation_at(&self, key: &str) -> Result<i64> {
    let mut conn = self.conn.as_ref().clone();
    let generation: i64 = measure_redis!(
        "redis.bump_devpost_ranking_generation",
        conn.incr::<_, _, i64>(key, 1_i64)
    )?;
    Ok(generation)
}

pub async fn get_devpost_ranking_generation(&self) -> Result<i64> {
    self.read_generation_at(&devpost_ranking_generation_key()).await
}

pub async fn bump_devpost_ranking_generation(&self) -> Result<i64> {
    self.bump_generation_at(&devpost_ranking_generation_key()).await
}

pub async fn set_devpost_ranking_response_for_generation(
    &self,
    generation: i64,
    page: i64,
    limit: i64,
    response: &RankingResponse,
) -> Result<()> {
    let mut conn = self.conn.as_ref().clone();
    let key = devpost_ranking_v2_key(generation, page, limit);
    let json = serde_json::to_string(response)?;
    measure_redis!(
        "redis.set_devpost_ranking_response_v2",
        conn.pset_ex::<String, String, ()>(key, json, *DEVPOST_RANKING_EXPIRATION)
    )?;
    Ok(())
}

pub async fn get_devpost_ranking_response_for_generation(
    &self,
    generation: i64,
    page: i64,
    limit: i64,
) -> Result<RankingResponse> {
    let mut conn = self.conn.as_ref().clone();
    let key = devpost_ranking_v2_key(generation, page, limit);
    let json: String = measure_redis!(
        "redis.get_devpost_ranking_response_v2",
        conn.get::<_, String>(key)
    )?;
    Ok(serde_json::from_str(&json)?)
}

pub async fn delete_devpost_trending(&self) -> Result<()> {
    let mut conn = self.conn.as_ref().clone();
    measure_redis!(
        "redis.delete_devpost_trending",
        conn.del::<String, ()>(devpost_trending_key())
    )?;
    Ok(())
}
```

Update existing trending get/set methods to call `devpost_trending_key()` so delete and cache use one exact key builder.

- [ ] **Step 4: Run the Redis primitive tests in GREEN**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test db::redis::devpost -- --nocapture
```

Expected: focused tests pass. The test process fails before connecting when the prefix is absent or is not UUID-scoped, and it never falls back to the `.env`/production `REDIS_URL`.

- [ ] **Step 5: Run formatting and the changed-source safety scan**

```bash
cargo fmt --all -- --check
git diff --check
git diff -- src/db/redis/mod.rs | rg "KEYS|SCAN|FLUSHALL|devpost:ranking:\{\}:\{\}" || true
```

Expected: focused tests pass; the scan prints no newly added forbidden enumeration and legacy methods remain untouched until Task 5.

- [ ] **Step 6: Commit**

```bash
git add src/db/redis/mod.rs
git commit -m "feat(dev-post): add ranking cache generations"
```

### Task 5: Switch ranking cache-aside to one captured generation and ignore legacy payloads

**Files:**
- Modify: `src/services/dev_post/mod.rs`
- Modify: `src/db/redis/mod.rs`
- Create: `src/services/dev_post/moderation_tests.rs` (initial ranking tests; expanded later)

**Interfaces:**
- Consumes: Task 4 generation-scoped Redis APIs.
- Produces: `get_ranking` reads generation once, bypasses cache on generation-read error, and never accesses legacy ranking payloads.

- [ ] **Step 1: Register a focused service test module and write RED tests**

At the bottom of `src/services/dev_post/mod.rs`, keep existing tests and add:

```rust
#[cfg(test)]
mod moderation_tests;
```

Create `src/services/dev_post/moderation_tests.rs` with shared setup and these tests:

```rust
use super::*;
use crate::db::{postgres::PostgresDatabase, redis::RedisDatabase};
use redis::AsyncCommands;
use std::sync::Arc;

fn assert_test_redis_namespace() {
    let raw = std::env::var("REDIS_KEY_PREFIX")
        .expect("Redis tests require an explicit REDIS_KEY_PREFIX");
    let uuid = raw
        .trim_end_matches(':')
        .strip_prefix("test-cms-dev-post-moderation-")
        .expect("refusing to touch Redis outside the CMS moderation test namespace");
    uuid::Uuid::parse_str(uuid).expect("REDIS_KEY_PREFIX must end in a per-process UUID");
    assert!(!crate::config::REDIS_KEY_PREFIX.is_empty());
}

fn exact_redis_key(suffix: impl AsRef<str>) -> String {
    assert_test_redis_namespace();
    format!(
        "{}{}",
        crate::config::REDIS_KEY_PREFIX.as_str(),
        suffix.as_ref()
    )
}

async fn test_redis() -> Arc<RedisDatabase> {
    assert_test_redis_namespace();
    Arc::new(RedisDatabase::new().await)
}

fn unique_checksum_address() -> String {
    let raw = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    crate::utils::valid_account_id(&format!("0x{}", &raw[..40])).unwrap()
}

async fn randomize_post_sequence(pool: &sqlx::PgPool) {
    let seed = ((uuid::Uuid::new_v4().as_u128() & ((1_u128 << 60) - 1)) as i64).max(1);
    let applied: i64 =
        sqlx::query_scalar("SELECT setval('dev_post_snowflake_seq'::regclass, $1, false)")
            .bind(seed)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(applied, seed);
}

fn service(pool: sqlx::PgPool, redis: Arc<RedisDatabase>) -> DevPostService {
    DevPostService::new(
        Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }),
        redis,
    )
}

async fn seed_ranked_post(pool: &sqlx::PgPool, token_id: &str) {
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'Ranked', 'RNK', 'img', $1, 0, '0xranked', 0)",
    )
    .bind(token_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dev_post (token_id, author, body) VALUES ($1, $1, 'ranked')")
        .bind(token_id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "./migrations-test")]
async fn ranking_generation_error_bypasses_get_and_set(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    assert_test_redis_namespace();
    randomize_post_sequence(&pool).await;
    let token = unique_checksum_address();
    seed_ranked_post(&pool, &token).await;
    let redis = test_redis().await;
    let page = (uuid::Uuid::new_v4().as_u128() % 1_000_000) as i64 + 1_000_000;
    let limit = 17_i64;
    let generation = redis.get_devpost_ranking_generation().await.unwrap();
    let v2_key = exact_redis_key(format!(
        "devpost:ranking:v2:{generation}:{page}:{limit}"
    ));
    let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    conn.del::<_, ()>(&v2_key).await.unwrap();
    let service = service(pool, redis);
    let (rows, total) = service
        .get_ranking_after_generation_read(page, limit, Err(anyhow::anyhow!("WRONGTYPE")))
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].token.token_id, token);
    assert_eq!(conn.exists::<_, i64>(&v2_key).await.unwrap(), 0);
}

#[sqlx::test(migrations = "./migrations-test")]
async fn ranking_reader_ignores_poisoned_legacy_payload(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    assert_test_redis_namespace();
    randomize_post_sequence(&pool).await;
    let token = unique_checksum_address();
    seed_ranked_post(&pool, &token).await;
    let redis = test_redis().await;
    let limit = (uuid::Uuid::new_v4().as_u128() % 1_000_000) as i64 + 8_000_000;
    let legacy_key = exact_redis_key(format!("devpost:ranking:1:{limit}"));
    let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    conn.pset_ex::<_, _, ()>(&legacy_key, "not-json", 60_000_u64)
        .await
        .unwrap();

    let generation = redis.get_devpost_ranking_generation().await.unwrap();
    let service = service(pool, redis.clone());
    let (rows, total) = service.get_ranking(1, limit).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].token.token_id, token);
    conn.del::<_, ()>(&legacy_key).await.unwrap();
    let v2_key = exact_redis_key(format!(
        "devpost:ranking:v2:{generation}:1:{limit}"
    ));
    conn.del::<_, ()>(&v2_key).await.unwrap();
}
```

- [ ] **Step 2: Run RED**

```bash
set -a
source .env
set +a
test -n "${REDIS_TEST_URL:-}" || {
  echo "REDIS_TEST_URL must point to a dedicated disposable non-production Redis" >&2
  exit 1
}
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::ranking -- --nocapture
```

Expected: compile failure because `get_ranking_after_generation_read` does not exist.

- [ ] **Step 3: Implement the captured-generation algorithm**

Replace `get_ranking` and add its private helper:

```rust
pub async fn get_ranking(
    &self,
    page: i64,
    limit: i64,
) -> Result<(Vec<RankingRow>, i64), AppError> {
    let generation = self.redis.get_devpost_ranking_generation().await;
    self.get_ranking_after_generation_read(page, limit, generation)
        .await
}

async fn get_ranking_after_generation_read(
    &self,
    page: i64,
    limit: i64,
    generation: anyhow::Result<i64>,
) -> Result<(Vec<RankingRow>, i64), AppError> {
    let generation = match generation {
        Ok(generation) => generation,
        Err(error) => {
            tracing::warn!(
                event = "dev_post.ranking.cache_bypass",
                reason = "generation_read_failed",
                error = %error
            );
            return DevPostController::new(self.postgres.clone())
                .get_ranking(page, limit)
                .await;
        }
    };
    if let Ok(cached) = self
        .redis
        .get_devpost_ranking_response_for_generation(generation, page, limit)
        .await
    {
        return Ok((cached.rankings, cached.total_count));
    }
    let (rankings, total_count) = DevPostController::new(self.postgres.clone())
        .get_ranking(page, limit)
        .await?;
    let response = RankingResponse {
        rankings,
        total_count,
    };
    if let Err(error) = self
        .redis
        .set_devpost_ranking_response_for_generation(generation, page, limit, &response)
        .await
    {
        tracing::warn!(
            event = "dev_post.ranking.cache_write_failed",
            generation,
            page,
            limit,
            error = %error
        );
    }
    Ok((response.rankings, response.total_count))
}
```

Remove the old `set_devpost_ranking_response(page, limit, ...)` and `get_devpost_ranking_response(page, limit)` methods from `src/db/redis/mod.rs`; after this deletion, new code has no legacy ranking payload reader or writer.

- [ ] **Step 4: Run ranking cache tests in GREEN**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::ranking -- --nocapture
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test db::redis::devpost -- --nocapture
```

Expected: both focused suites pass.

- [ ] **Step 5: Verify the legacy ranking methods are absent**

```bash
rg -n "get_devpost_ranking_response\(|set_devpost_ranking_response\(" src || true
cargo fmt --all -- --check
```

Expected: focused tests pass and `rg` prints no old method call/definition.

- [ ] **Step 6: Commit**

```bash
git add src/db/redis/mod.rs src/services/dev_post/mod.rs src/services/dev_post/moderation_tests.rs
git commit -m "refactor(dev-post): scope ranking cache by generation"
```

### Task 6: Extract the shared soft-delete core and preserve author behavior

**Files:**
- Create: `src/controllers/dev_post/moderation.rs`
- Create: `src/controllers/dev_post/moderation_tests.rs`
- Modify: `src/controllers/dev_post/mod.rs`
- Read: `src/controllers/dev_post/pin.rs`

**Interfaces:**
- Produces `PostMutationContext { post_id: i64, token_id: String, changed: bool }`.
- Produces `CommitOutcome<T>::Committed(T)` and `CommitOutcome<T>::Unknown { context: T, error: String }`.
- Produces `delete_post_as_author(&self, post_id: i64, author: &str) -> Result<CommitOutcome<PostMutationContext>, AppError>`.
- Keeps the existing `delete_post(...)->Result<String, AppError>` as a temporary compatibility adapter until Task 8 switches the service.

- [ ] **Step 1: Register the module and add failing author-policy tests**

In `src/controllers/dev_post/mod.rs`, add:

```rust
mod moderation;
#[cfg(test)]
mod moderation_tests;

pub(crate) use moderation::{
    CommitOutcome, ModerationAction, ModerationAudit, ModerationContext, PostMutationContext,
};
```

Create `src/controllers/dev_post/moderation_tests.rs` with shared seed helpers and the author test:

```rust
use super::*;
use crate::db::postgres::PostgresDatabase;
use std::sync::Arc;

pub(super) const ADMIN: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
pub(super) const ADMIN_CASE_MUTATED: &str = "0x52908400098527886e0f7030069857d2e4169ee7";
pub(super) const OTHER: &str = "0xde709f2102306220921060314715629080e2fb77";
pub(super) const TOKEN: &str = "0x0000000000000000000000000000000000006000";

pub(super) fn controller(pool: sqlx::PgPool) -> DevPostController {
    DevPostController::new(Arc::new(PostgresDatabase {
        write_pool: pool.clone(),
        read_pool: pool,
    }))
}

pub(super) async fn seed_token_and_post(pool: &sqlx::PgPool) -> i64 {
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'Moderation', 'MOD', 'img', $2, 0, '0x6000', 0)",
    )
    .bind(TOKEN)
    .bind(ADMIN)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'post') RETURNING id",
    )
    .bind(TOKEN)
    .bind(ADMIN)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub(super) fn committed<T>(outcome: CommitOutcome<T>) -> T {
    match outcome {
        CommitOutcome::Committed(context) => context,
        CommitOutcome::Unknown { error, .. } => panic!("unexpected commit error: {error}"),
    }
}

#[sqlx::test(migrations = "./migrations-test")]
async fn author_delete_keeps_contract_unpins_and_writes_no_cms_audit(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(TOKEN)
        .bind(post_id)
        .execute(&pool)
        .await
        .unwrap();
    let controller = controller(pool.clone());

    let context = committed(
        controller
            .delete_post_as_author(post_id, ADMIN)
            .await
            .unwrap(),
    );
    assert_eq!(context.post_id, post_id);
    assert_eq!(context.token_id, TOKEN);
    assert!(context.changed);
    let pin_count: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pin_count, 0);
    assert_eq!(audit_count, 0);

    assert!(matches!(
        controller.delete_post_as_author(post_id, ADMIN).await,
        Err(AppError::NotFound(_))
    ));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn author_delete_wrong_author_is_forbidden(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    assert!(matches!(
        controller(pool)
            .delete_post_as_author(post_id, OTHER)
            .await,
        Err(AppError::Forbidden(_))
    ));
}
```

- [ ] **Step 2: Run RED**

```bash
set -a
source .env
set +a
cargo test controllers::dev_post::moderation_tests::author_delete -- --nocapture
```

Expected: compile failure because the moderation types and `delete_post_as_author` do not exist.

- [ ] **Step 3: Implement the shared context, commit outcome, and delete primitive**

Create `src/controllers/dev_post/moderation.rs` with these definitions and author entry point:

```rust
use super::{DevPostController, pin};
use crate::result::AppError;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostMutationContext {
    pub(crate) post_id: i64,
    pub(crate) token_id: String,
    pub(crate) changed: bool,
}

#[derive(Debug)]
pub(crate) enum CommitOutcome<T> {
    Committed(T),
    Unknown { context: T, error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModerationAction {
    Delete,
    Restore,
}

impl ModerationAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Restore => "RESTORE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModerationContext {
    pub(crate) audit_id: Uuid,
    pub(crate) admin_account_id: String,
    pub(crate) post_id: i64,
    pub(crate) token_id: String,
    pub(crate) action: ModerationAction,
    pub(crate) changed: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ModerationAudit {
    pub(crate) id: Uuid,
    pub(crate) post_id: i64,
    pub(crate) token_id: String,
    pub(crate) admin_account_id: String,
    pub(crate) action: String,
    pub(crate) changed: bool,
}

impl ModerationAudit {
    pub(crate) fn matches(&self, context: &ModerationContext) -> bool {
        self.id == context.audit_id
            && self.post_id == context.post_id
            && self.token_id == context.token_id
            && self.admin_account_id == context.admin_account_id
            && self.action == context.action.as_str()
            && self.changed == context.changed
    }
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::InternalError(error.to_string())
}

async fn apply_soft_delete(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
) -> Result<bool, AppError> {
    sqlx::query("DELETE FROM dev_post_pin WHERE post_id = $1")
        .bind(post_id)
        .execute(&mut **tx)
        .await
        .map_err(internal)?;
    let result = sqlx::query(
        "UPDATE dev_post SET deleted_at = clock_timestamp() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(post_id)
    .execute(&mut **tx)
    .await
    .map_err(internal)?;
    Ok(result.rows_affected() == 1)
}

async fn finish_transaction<T>(
    tx: Transaction<'_, Postgres>,
    context: T,
) -> Result<CommitOutcome<T>, AppError> {
    match tx.commit().await {
        Ok(()) => Ok(CommitOutcome::Committed(context)),
        Err(error) => Ok(CommitOutcome::Unknown {
            context,
            error: error.to_string(),
        }),
    }
}

impl DevPostController {
    pub(crate) async fn delete_post_as_author(
        &self,
        post_id: i64,
        author: &str,
    ) -> Result<CommitOutcome<PostMutationContext>, AppError> {
        let mut tx = self.db.get_write_pool().begin().await.map_err(internal)?;
        let pending = async {
            let locked = pin::lock_live_post_context(&mut tx, post_id).await?;
            if locked.author != author {
                return Err(AppError::Forbidden("Not the post author".into()));
            }
            let changed = apply_soft_delete(&mut tx, post_id).await?;
            Ok(PostMutationContext {
                post_id,
                token_id: locked.token_id,
                changed,
            })
        }
        .await;
        let context = match pending {
            Ok(context) => context,
            Err(error) => {
                let _ = tx.rollback().await;
                return Err(error);
            }
        };
        finish_transaction(tx, context).await
    }

    pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<String, AppError> {
        match self.delete_post_as_author(post_id, author).await? {
            CommitOutcome::Committed(context) => Ok(context.token_id),
            CommitOutcome::Unknown { .. } => {
                Err(AppError::InternalError("outcome_unknown".into()))
            }
        }
    }
}
```

Remove the old inline `delete_post` transaction from `src/controllers/dev_post/mod.rs`. Reuse `pin::lock_live_post_context` without changing its token `FOR UPDATE`, post `FOR UPDATE`, creator, or author behavior; CMS uses its own `FOR KEY SHARE` token lookup in Task 7.

- [ ] **Step 4: Run GREEN and the existing pin/delete suite**

```bash
cargo test controllers::dev_post::moderation_tests::author_delete -- --nocapture
cargo test controllers::dev_post::pin -- --nocapture
cargo test controllers::dev_post::pin_feed_tests -- --nocapture
cargo fmt --all -- --check
```

Expected: all focused tests pass; existing author response behavior and pin behavior remain green.

- [ ] **Step 5: Commit**

```bash
git add src/controllers/dev_post/mod.rs src/controllers/dev_post/moderation.rs src/controllers/dev_post/moderation_tests.rs
git commit -m "refactor(dev-post): share atomic soft-delete core"
```

### Task 7: Implement admin-first audited delete and restore transactions

**Files:**
- Modify: `src/controllers/dev_post/moderation.rs`
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: `CommitOutcome`, `ModerationContext`, `apply_soft_delete`, and Task 3 schema.
- Produces:
  - `delete_post_as_admin(&self, post_id: i64, admin: &str, audit_id: Uuid) -> Result<CommitOutcome<ModerationContext>, AppError>`
  - `restore_post_as_admin(&self, post_id: i64, admin: &str, audit_id: Uuid) -> Result<CommitOutcome<ModerationContext>, AppError>`
  - `get_moderation_audit_on_write_pool(&self, audit_id: Uuid) -> Result<Option<ModerationAudit>, AppError>`

- [ ] **Step 1: Add failing CMS authorization/idempotency/orphan tests**

Append these focused tests, using the helpers from Task 6:

```rust
async fn seed_admin(pool: &sqlx::PgPool, account_id: &str) {
    sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn audit_rows(pool: &sqlx::PgPool, post_id: i64) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT action, changed FROM dev_post_moderation_log \
         WHERE post_id = $1 ORDER BY created_at, id",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations-test")]
async fn cms_delete_and_restore_are_idempotent_and_each_attempt_is_audited(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let controller = controller(pool.clone());

    let first_delete = committed(
        controller
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    let repeated_delete = committed(
        controller
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    assert!(first_delete.changed);
    assert!(!repeated_delete.changed);

    let first_restore = committed(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    let repeated_restore = committed(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    assert!(first_restore.changed);
    assert!(!repeated_restore.changed);
    assert_eq!(
        audit_rows(&pool, post_id).await,
        vec![
            ("DELETE".into(), true),
            ("DELETE".into(), false),
            ("RESTORE".into(), true),
            ("RESTORE".into(), false),
        ]
    );
}

#[sqlx::test(migrations = "./migrations-test")]
async fn cms_checks_exact_admin_before_revealing_post_existence(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let controller = controller(pool.clone());
    for target in [post_id, i64::MAX] {
        assert!(matches!(
            controller
                .delete_post_as_admin(target, OTHER, uuid::Uuid::new_v4())
                .await,
            Err(AppError::Forbidden(_))
        ));
    }
    assert!(matches!(
        controller
            .delete_post_as_admin(post_id, ADMIN_CASE_MUTATED, uuid::Uuid::new_v4())
            .await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        controller
            .delete_post_as_admin(i64::MAX, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        controller
            .restore_post_as_admin(i64::MAX, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::NotFound(_))
    ));
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audits, 0);
}

#[sqlx::test(migrations = "./migrations-test")]
async fn orphan_delete_succeeds_but_orphan_restore_conflicts_without_audit(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    sqlx::query("DELETE FROM token WHERE token_id = $1")
        .bind(TOKEN)
        .execute(&pool)
        .await
        .unwrap();
    let controller = controller(pool.clone());
    assert!(matches!(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::Conflict(_))
    ));
    assert!(audit_rows(&pool, post_id).await.is_empty());
    assert!(
        committed(
            controller
                .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
                .await
                .unwrap()
        )
        .changed
    );
    assert!(
        !committed(
            controller
                .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
                .await
                .unwrap()
        )
        .changed
    );
    assert!(matches!(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        audit_rows(&pool, post_id).await,
        vec![("DELETE".into(), true), ("DELETE".into(), false)]
    );
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test controllers::dev_post::moderation_tests::cms_ -- --nocapture
cargo test controllers::dev_post::moderation_tests::orphan_ -- --nocapture
```

Expected: compile failure naming the missing admin methods.

- [ ] **Step 3: Implement admin-first locking, action policy, audit, and reconciliation lookup**

Add to `moderation.rs`:

```rust
#[derive(sqlx::FromRow)]
struct LockedModerationPost {
    was_deleted: bool,
}

async fn lock_admin(
    tx: &mut Transaction<'_, Postgres>,
    admin: &str,
) -> Result<(), AppError> {
    let row: Option<String> = sqlx::query_scalar(
        "SELECT account_id FROM admin WHERE account_id = $1 FOR KEY SHARE",
    )
    .bind(admin)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?;
    row.ok_or_else(|| AppError::Forbidden("Admin access required".into()))?;
    Ok(())
}

async fn insert_audit(
    tx: &mut Transaction<'_, Postgres>,
    context: &ModerationContext,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(context.audit_id)
    .bind(context.post_id)
    .bind(&context.token_id)
    .bind(&context.admin_account_id)
    .bind(context.action.as_str())
    .bind(context.changed)
    .execute(&mut **tx)
    .await
    .map_err(internal)?;
    Ok(())
}

impl DevPostController {
    pub(crate) async fn delete_post_as_admin(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: Uuid,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        self.moderate_post_as_admin(post_id, admin, audit_id, ModerationAction::Delete)
            .await
    }

    pub(crate) async fn restore_post_as_admin(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: Uuid,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        self.moderate_post_as_admin(post_id, admin, audit_id, ModerationAction::Restore)
            .await
    }

    async fn moderate_post_as_admin(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: Uuid,
        action: ModerationAction,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        let mut tx = self.db.get_write_pool().begin().await.map_err(internal)?;
        let pending = async {
            lock_admin(&mut tx, admin).await?;
            let token_id: Option<String> =
                sqlx::query_scalar("SELECT token_id FROM dev_post WHERE id = $1")
                    .bind(post_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(internal)?;
            let token_id =
                token_id.ok_or_else(|| AppError::NotFound("Post not found".into()))?;

            let locked_token: Option<String> = sqlx::query_scalar(
                "SELECT token_id FROM token WHERE token_id = $1 FOR KEY SHARE",
            )
            .bind(&token_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;
            if action == ModerationAction::Restore && locked_token.is_none() {
                return Err(AppError::Conflict(
                    "Cannot restore a post whose token no longer exists".into(),
                ));
            }

            let post: Option<LockedModerationPost> = sqlx::query_as(
                "SELECT (deleted_at IS NOT NULL) AS was_deleted FROM dev_post \
                 WHERE id = $1 AND token_id = $2 FOR UPDATE",
            )
            .bind(post_id)
            .bind(&token_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;
            let post = post.ok_or_else(|| AppError::NotFound("Post not found".into()))?;

            let changed = match action {
                ModerationAction::Delete => apply_soft_delete(&mut tx, post_id).await?,
                ModerationAction::Restore if post.was_deleted => {
                    sqlx::query("DELETE FROM dev_post_pin WHERE post_id = $1")
                        .bind(post_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(internal)?;
                    let result = sqlx::query(
                        "UPDATE dev_post SET deleted_at = NULL \
                         WHERE id = $1 AND deleted_at IS NOT NULL",
                    )
                    .bind(post_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(internal)?;
                    result.rows_affected() == 1
                }
                ModerationAction::Restore => false,
            };
            let context = ModerationContext {
                audit_id,
                admin_account_id: admin.to_string(),
                post_id,
                token_id,
                action,
                changed,
            };
            insert_audit(&mut tx, &context).await?;
            Ok(context)
        }
        .await;
        let context = match pending {
            Ok(context) => context,
            Err(error) => {
                let _ = tx.rollback().await;
                return Err(error);
            }
        };
        finish_transaction(tx, context).await
    }

    pub(crate) async fn get_moderation_audit_on_write_pool(
        &self,
        audit_id: Uuid,
    ) -> Result<Option<ModerationAudit>, AppError> {
        sqlx::query_as(
            "SELECT id, post_id, token_id, admin_account_id, action, changed \
             FROM dev_post_moderation_log WHERE id = $1",
        )
        .bind(audit_id)
        .fetch_optional(self.db.get_write_pool())
        .await
        .map_err(internal)
    }
}
```

- [ ] **Step 4: Run controller moderation tests in GREEN**

```bash
cargo test controllers::dev_post::moderation_tests -- --nocapture
```

Expected: all moderation tests pass.

- [ ] **Step 5: Verify exact-address SQL and formatting**

```bash
rg -n "LOWER\(|ILIKE|lower\(" src/controllers/dev_post/moderation.rs && exit 1 || true
cargo fmt --all -- --check
```

Expected: all moderation tests pass and the address scan finds nothing.

- [ ] **Step 6: Commit**

```bash
git add src/controllers/dev_post/moderation.rs src/controllers/dev_post/moderation_tests.rs
git commit -m "feat(dev-post): add audited admin delete and restore"
```

### Task 8: Orchestrate commit reconciliation, outcome-unknown handling, and full invalidation

**Files:**
- Modify: `src/services/dev_post/mod.rs`
- Modify: `src/services/dev_post/moderation_tests.rs`
- Modify: `src/controllers/dev_post/moderation.rs`

**Interfaces:**
- Consumes: Task 7 controller outcomes and audit lookup; Task 4 Redis invalidation methods.
- Produces:
  - `delete_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError>`
  - `restore_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError>`
  - author `delete_post` now consumes `delete_post_as_author` and invalidates even on ambiguous commit.
  - one shared `tokio::join!` invalidator whose failures are isolated and logged.

- [ ] **Step 1: Write failing reconciliation and author-outcome tests**

Append to `src/services/dev_post/moderation_tests.rs`:

```rust
fn cms_context(action: ModerationAction, changed: bool) -> ModerationContext {
    ModerationContext {
        audit_id: uuid::Uuid::new_v4(),
        admin_account_id: "0x52908400098527886E0F7030069857D2E4169EE7".into(),
        post_id: 81,
        token_id: "0x0000000000000000000000000000000000000081".into(),
        action,
        changed,
    }
}

fn matching_audit(context: &ModerationContext) -> ModerationAudit {
    ModerationAudit {
        id: context.audit_id,
        post_id: context.post_id,
        token_id: context.token_id.clone(),
        admin_account_id: context.admin_account_id.clone(),
        action: context.action.as_str().into(),
        changed: context.changed,
    }
}

#[sqlx::test(migrations = "./migrations-test")]
async fn exact_audit_match_reconciles_commit_error(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let service = service(pool.clone(), test_redis().await);
    let context = cms_context(ModerationAction::Delete, true);
    let audit = matching_audit(&context);
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id, post_id, token_id, admin_account_id, action, changed) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(audit.id)
    .bind(audit.post_id)
    .bind(&audit.token_id)
    .bind(&audit.admin_account_id)
    .bind(&audit.action)
    .bind(audit.changed)
    .execute(&pool)
    .await
    .unwrap();
    let controller = DevPostController::new(service.postgres.clone());
    let result = service
        .finish_cms_moderation(
            &controller,
            CommitOutcome::Unknown {
                context,
                error: "connection lost after COMMIT".into(),
            },
        )
        .await;
    assert!(result.is_ok());
}

#[sqlx::test(migrations = "./migrations-test")]
async fn missing_mismatched_and_failed_reconciliation_remain_outcome_unknown(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let service = service(pool, test_redis().await);
    let context = cms_context(ModerationAction::Restore, false);
    let mut mismatched = matching_audit(&context);
    mismatched.action = "DELETE".into();
    let cases = vec![
        Ok(None),
        Ok(Some(mismatched)),
        Err(AppError::InternalError("write pool unavailable".into())),
    ];
    for lookup in cases {
        let error = service
            .finish_cms_unknown_after_lookup(
                context.clone(),
                "connection lost after COMMIT".into(),
                lookup,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InternalError(message) if message == "outcome_unknown"));
    }
}

#[sqlx::test(migrations = "./migrations-test")]
async fn author_commit_error_is_outcome_unknown_without_audit_lookup(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let service = service(pool, test_redis().await);
    let outcome = CommitOutcome::Unknown {
        context: PostMutationContext {
            post_id: 82,
            token_id: "0x0000000000000000000000000000000000000082".into(),
            changed: true,
        },
        error: "connection lost after COMMIT".into(),
    };
    let error = service.finish_author_delete(outcome).await.unwrap_err();
    assert!(matches!(error, AppError::InternalError(message) if message == "outcome_unknown"));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn retries_are_safe_for_both_possible_outcome_unknown_states(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let admin = "0x52908400098527886E0F7030069857D2E4169EE7";
    let token = "0x0000000000000000000000000000000000008088";
    seed_ranked_post(&pool, token).await;
    sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let committed_post: i64 = sqlx::query_scalar("SELECT id FROM dev_post WHERE token_id = $1")
        .bind(token)
        .fetch_one(&pool)
        .await
        .unwrap();
    let uncommitted_post: i64 = sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $1, 'second') RETURNING id",
    )
    .bind(token)
    .fetch_one(&pool)
    .await
    .unwrap();
    let redis = test_redis().await;
    let service = service(pool.clone(), redis);
    let controller = DevPostController::new(service.postgres.clone());

    let first_context = match controller
        .delete_post_as_admin(committed_post, admin, uuid::Uuid::new_v4())
        .await
        .unwrap()
    {
        CommitOutcome::Committed(context) => context,
        CommitOutcome::Unknown { error, .. } => panic!("unexpected real commit error: {error}"),
    };
    assert!(service
        .finish_cms_unknown_after_lookup(
            first_context,
            "simulated lost acknowledgement".into(),
            Ok(None),
        )
        .await
        .is_err());
    service
        .delete_post_as_admin(committed_post, admin)
        .await
        .unwrap();

    let imaginary_context = ModerationContext {
        audit_id: uuid::Uuid::new_v4(),
        admin_account_id: admin.into(),
        post_id: uncommitted_post,
        token_id: token.into(),
        action: ModerationAction::Delete,
        changed: true,
    };
    assert!(service
        .finish_cms_unknown_after_lookup(
            imaginary_context,
            "simulated rollback before server commit".into(),
            Ok(None),
        )
        .await
        .is_err());
    service
        .delete_post_as_admin(uncommitted_post, admin)
        .await
        .unwrap();

    let changed: Vec<bool> = sqlx::query_scalar(
        "SELECT changed FROM dev_post_moderation_log \
         WHERE post_id = ANY($1) ORDER BY post_id, created_at",
    )
    .bind(vec![committed_post, uncommitted_post])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(changed.iter().filter(|value| **value).count(), 2);
    assert_eq!(changed.iter().filter(|value| !**value).count(), 1);
}
```

- [ ] **Step 2: Run RED**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::exact_audit -- --nocapture --test-threads=1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::missing_mismatched -- --nocapture --test-threads=1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::author_commit_error -- --nocapture --test-threads=1
```

Expected: compile failures naming the missing service completion helpers.

- [ ] **Step 3: Implement service entry points and commit-boundary handling**

Import `std::future::Future`, `uuid::Uuid`, controller outcome types, and tracing macros. Replace author `delete_post` and add CMS methods:

```rust
pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
    let outcome = DevPostController::new(self.postgres.clone())
        .delete_post_as_author(post_id, author)
        .await?;
    self.finish_author_delete(outcome).await
}

pub async fn delete_post_as_admin(
    &self,
    post_id: i64,
    admin: &str,
) -> Result<(), AppError> {
    self.validate_moderation_post_id(post_id)?;
    let controller = DevPostController::new(self.postgres.clone());
    let outcome = controller
        .delete_post_as_admin(post_id, admin, Uuid::new_v4())
        .await?;
    self.finish_cms_moderation(&controller, outcome).await
}

pub async fn restore_post_as_admin(
    &self,
    post_id: i64,
    admin: &str,
) -> Result<(), AppError> {
    self.validate_moderation_post_id(post_id)?;
    let controller = DevPostController::new(self.postgres.clone());
    let outcome = controller
        .restore_post_as_admin(post_id, admin, Uuid::new_v4())
        .await?;
    self.finish_cms_moderation(&controller, outcome).await
}

fn validate_moderation_post_id(&self, post_id: i64) -> Result<(), AppError> {
    if post_id <= 0 {
        return Err(AppError::BadRequest(
            "post_id must be a positive BIGINT".into(),
        ));
    }
    Ok(())
}

async fn finish_author_delete(
    &self,
    outcome: CommitOutcome<PostMutationContext>,
) -> Result<(), AppError> {
    match outcome {
        CommitOutcome::Committed(context) => {
            self.invalidate_post_public_caches(context.post_id, &context.token_id)
                .await;
            Ok(())
        }
        CommitOutcome::Unknown { context, error } => {
            self.invalidate_post_public_caches(context.post_id, &context.token_id)
                .await;
            tracing::error!(
                event = "dev_post.author_delete.outcome_unknown",
                post_id = context.post_id,
                token_id = %context.token_id,
                commit_error = %error
            );
            Err(AppError::InternalError("outcome_unknown".into()))
        }
    }
}

async fn finish_cms_moderation(
    &self,
    controller: &DevPostController,
    outcome: CommitOutcome<ModerationContext>,
) -> Result<(), AppError> {
    match outcome {
        CommitOutcome::Committed(context) => {
            self.finish_confirmed_cms_moderation(context, false).await;
            Ok(())
        }
        CommitOutcome::Unknown { context, error } => {
            let lookup = controller
                .get_moderation_audit_on_write_pool(context.audit_id)
                .await;
            self.finish_cms_unknown_after_lookup(context, error, lookup)
                .await
        }
    }
}

async fn finish_cms_unknown_after_lookup(
    &self,
    context: ModerationContext,
    commit_error: String,
    lookup: Result<Option<ModerationAudit>, AppError>,
) -> Result<(), AppError> {
    self.finish_cms_unknown_after_lookup_with_bump(
        context,
        commit_error,
        lookup,
        self.redis.bump_devpost_ranking_generation(),
    )
    .await
}

async fn finish_cms_unknown_after_lookup_with_bump<F>(
    &self,
    context: ModerationContext,
    commit_error: String,
    lookup: Result<Option<ModerationAudit>, AppError>,
    ranking_bump: F,
) -> Result<(), AppError>
where
    F: Future<Output = anyhow::Result<i64>>,
{
    if matches!(lookup.as_ref(), Ok(Some(audit)) if audit.matches(&context)) {
        self.finish_confirmed_cms_moderation_with_bump(context, true, ranking_bump)
            .await;
        return Ok(());
    }
    self.invalidate_post_public_caches_with_bump(
        context.post_id,
        &context.token_id,
        ranking_bump,
    )
        .await;
    tracing::error!(
        event = "cms.dev_post.moderation.outcome_unknown",
        audit_id = %context.audit_id,
        admin_account_id = %context.admin_account_id,
        post_id = context.post_id,
        token_id = %context.token_id,
        action = context.action.as_str(),
        commit_error = %commit_error,
        reconciliation = ?lookup
    );
    Err(AppError::InternalError("outcome_unknown".into()))
}

async fn finish_confirmed_cms_moderation(
    &self,
    context: ModerationContext,
    commit_reconciled: bool,
) {
    self.finish_confirmed_cms_moderation_with_bump(
        context,
        commit_reconciled,
        self.redis.bump_devpost_ranking_generation(),
    )
    .await;
}

async fn finish_confirmed_cms_moderation_with_bump<F>(
    &self,
    context: ModerationContext,
    commit_reconciled: bool,
    ranking_bump: F,
) where
    F: Future<Output = anyhow::Result<i64>>,
{
    self.invalidate_post_public_caches_with_bump(
        context.post_id,
        &context.token_id,
        ranking_bump,
    )
        .await;
    tracing::info!(
        event = "cms.dev_post.moderation",
        audit_id = %context.audit_id,
        admin_account_id = %context.admin_account_id,
        post_id = context.post_id,
        token_id = %context.token_id,
        action = context.action.as_str(),
        changed = context.changed,
        commit_reconciled
    );
}
```

- [ ] **Step 4: Implement one independently attempted full invalidation helper**

Add this helper. The future parameter makes the ranking-failure branch directly testable without poisoning a shared production generation key:

```rust
async fn invalidate_post_public_caches(&self, post_id: i64, token_id: &str) {
    self.invalidate_post_public_caches_with_bump(
        post_id,
        token_id,
        self.redis.bump_devpost_ranking_generation(),
    )
    .await;
}

async fn invalidate_post_public_caches_with_bump<F>(
    &self,
    post_id: i64,
    token_id: &str,
    ranking_bump: F,
) where
    F: Future<Output = anyhow::Result<i64>>,
{
    let (global_feed, token_feed, detail, trending, ranking) = tokio::join!(
        self.redis.delete_devpost_feed("global"),
        self.redis.delete_devpost_feed(token_id),
        self.redis.delete_devpost_detail(post_id),
        self.redis.delete_devpost_trending(),
        ranking_bump,
    );
    for (family, result) in [
        ("global_feed", global_feed),
        ("token_feed", token_feed),
        ("detail", detail),
        ("trending", trending),
    ] {
        if let Err(error) = result {
            tracing::warn!(
                event = "dev_post.cache_invalidation_failed",
                family,
                post_id,
                token_id,
                error = %error
            );
        }
    }
    if let Err(error) = ranking {
        tracing::warn!(
            event = "dev_post.cache_invalidation_failed",
            family = "ranking_generation",
            post_id,
            token_id,
            error = %error
        );
    }
}
```

In `src/controllers/dev_post/moderation.rs`, put the compatibility `delete_post` adapter behind `#[cfg(test)]`; production service code must use `delete_post_as_author` so commit-unknown invalidation cannot be bypassed.

- [ ] **Step 5: Run service/controller completion tests in GREEN**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests -- --nocapture --test-threads=1
cargo test controllers::dev_post -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: focused controller/service tests pass; all logged fields are identifiers/state only and no body, image, cookie, session, authorization header, request body, or IP is logged.

- [ ] **Step 6: Commit the completion and invalidation implementation**

```bash
git add src/controllers/dev_post/moderation.rs src/services/dev_post/mod.rs src/services/dev_post/moderation_tests.rs
git commit -m "feat(dev-post): reconcile moderation commits and invalidate caches"
```

### Task 9: Prove complete invalidation, no-op repair, and ranking-bump failure isolation

**Files:**
- Modify: `src/services/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: Task 8 shared invalidator and public CMS service methods.
- Produces: serialized integration proof that author delete, CMS delete/restore, CMS no-ops, and outcome-unknown completion remove all six exact payload keys; one failed bump cannot skip the other four families or replace `outcome_unknown`.

- [ ] **Step 1: Add the serialized exact-key cache fixture**

Add these helpers to `src/services/dev_post/moderation_tests.rs`. The test captures a live detail once and may reuse that intentionally stale value after deletion; it never calls `get_post_base` while the post is deleted. `assert_test_redis_namespace` and the UUID-scoped process prefix are the cross-process isolation boundary. The module lock only serializes tests sharing that already-isolated process namespace; it does not make a shared or production Redis safe and does not protect router tests launched by another Cargo process.

```rust
use std::sync::OnceLock;

static REDIS_SINGLETON_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn redis_singleton_lock() -> &'static tokio::sync::Mutex<()> {
    REDIS_SINGLETON_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn prefixed(key: impl AsRef<str>) -> String {
    exact_redis_key(key)
}

struct SeededPublicCaches {
    payload_keys: [String; 6],
    generation_before: i64,
}

async fn seed_public_caches(
    redis: &RedisDatabase,
    post_id: i64,
    token_id: &str,
    captured_live_detail: &DevPostResponse,
) -> SeededPublicCaches {
    assert_test_redis_namespace();
    let payload_keys = [
        prefixed("devpost:feed:global"),
        prefixed("devpost:feed:v2:global"),
        prefixed(format!("devpost:feed:{token_id}")),
        prefixed(format!("devpost:feed:v2:{token_id}")),
        prefixed(format!("devpost:detail:{post_id}")),
        prefixed("devpost:trending"),
    ];
    let feed = FeedBase {
        pin: None,
        posts: Vec::new(),
        total_count: 99,
    };
    let feed_json = serde_json::to_string(&feed).unwrap();
    let detail_json = serde_json::to_string(captured_live_detail).unwrap();
    let trending_json = serde_json::to_string(&[captured_live_detail]).unwrap();
    let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    for key in &payload_keys[..4] {
        conn.pset_ex::<_, _, ()>(key, &feed_json, 60_000_u64)
            .await
            .unwrap();
    }
    conn.pset_ex::<_, _, ()>(&payload_keys[4], detail_json, 60_000_u64)
        .await
        .unwrap();
    conn.pset_ex::<_, _, ()>(&payload_keys[5], trending_json, 60_000_u64)
        .await
        .unwrap();
    for key in &payload_keys {
        assert_eq!(conn.exists::<_, i64>(key).await.unwrap(), 1, "not seeded: {key}");
    }
    SeededPublicCaches {
        payload_keys,
        generation_before: redis.get_devpost_ranking_generation().await.unwrap(),
    }
}

async fn assert_public_caches_cleared(
    redis: &RedisDatabase,
    seeded: &SeededPublicCaches,
    expect_generation_bump: bool,
) {
    let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    for key in &seeded.payload_keys {
        assert_eq!(conn.exists::<_, i64>(key).await.unwrap(), 0, "not deleted: {key}");
    }
    let generation_after = redis.get_devpost_ranking_generation().await.unwrap();
    if expect_generation_bump {
        assert!(generation_after > seeded.generation_before);
    } else {
        assert_eq!(generation_after, seeded.generation_before);
    }
}
```

- [ ] **Step 2: Add the author/CMS confirmed-outcome conformance test**

Add this complete test. `unique_checksum_address()` and `randomize_post_sequence()` are the exact helpers defined in Task 5.

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn author_and_cms_mutations_clear_every_public_cache_family(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let _serial = redis_singleton_lock().lock().await;
    let admin = "0x52908400098527886E0F7030069857D2E4169EE7";
    randomize_post_sequence(&pool).await;
    let token = unique_checksum_address();
    seed_ranked_post(&pool, &token).await;
    sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let post_id: i64 = sqlx::query_scalar("SELECT id FROM dev_post WHERE token_id = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .unwrap();
    let redis = test_redis().await;
    let service = service(pool.clone(), redis.clone());
    let live_detail = DevPostController::new(service.postgres.clone())
        .get_post_base(post_id)
        .await
        .unwrap();

    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    service.delete_post(post_id, &token).await.unwrap();
    assert_public_caches_cleared(&redis, &seeded, true).await;

    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    service.restore_post_as_admin(post_id, admin).await.unwrap();
    assert_public_caches_cleared(&redis, &seeded, true).await;

    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    service.restore_post_as_admin(post_id, admin).await.unwrap();
    assert_public_caches_cleared(&redis, &seeded, true).await;

    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    service.delete_post_as_admin(post_id, admin).await.unwrap();
    assert_public_caches_cleared(&redis, &seeded, true).await;

    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    service.delete_post_as_admin(post_id, admin).await.unwrap();
    assert_public_caches_cleared(&redis, &seeded, true).await;
}
```

- [ ] **Step 3: Add outcome-unknown and injected-bump completion-path conformance tests**

Add both complete tests. They enter through `finish_cms_unknown_after_lookup*`, not the invalidator directly. The unmatched lookup proves `outcome_unknown`; the exact-audit match proves a confirmed CMS completion still returns success when the ranking bump fails, while every payload family is cleared.

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn unknown_commit_outcome_invalidates_seeded_exact_keys(pool: sqlx::PgPool) {
    dotenv::dotenv().ok();
    let _serial = redis_singleton_lock().lock().await;
    randomize_post_sequence(&pool).await;
    let token = unique_checksum_address();
    seed_ranked_post(&pool, &token).await;
    let post_id: i64 = sqlx::query_scalar("SELECT id FROM dev_post WHERE token_id = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .unwrap();
    let redis = test_redis().await;
    let service = service(pool, redis.clone());
    let live_detail = DevPostController::new(service.postgres.clone())
        .get_post_base(post_id)
        .await
        .unwrap();
    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    let context = ModerationContext {
        audit_id: uuid::Uuid::new_v4(),
        admin_account_id: unique_checksum_address(),
        post_id,
        token_id: token,
        action: ModerationAction::Delete,
        changed: true,
    };

    let error = service
        .finish_cms_unknown_after_lookup(
            context,
            "connection lost after COMMIT".into(),
            Ok(None),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InternalError(message) if message == "outcome_unknown"));
    assert_public_caches_cleared(&redis, &seeded, true).await;
}

#[sqlx::test(migrations = "./migrations-test")]
async fn confirmed_cms_completion_survives_ranking_bump_failure_and_clears_payloads(
    pool: sqlx::PgPool,
) {
    dotenv::dotenv().ok();
    let _serial = redis_singleton_lock().lock().await;
    randomize_post_sequence(&pool).await;
    let token = unique_checksum_address();
    seed_ranked_post(&pool, &token).await;
    let post_id: i64 = sqlx::query_scalar("SELECT id FROM dev_post WHERE token_id = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .unwrap();
    let redis = test_redis().await;
    let service = service(pool, redis.clone());
    let live_detail = DevPostController::new(service.postgres.clone())
        .get_post_base(post_id)
        .await
        .unwrap();
    let seeded = seed_public_caches(&redis, post_id, &token, &live_detail).await;
    let context = ModerationContext {
        audit_id: uuid::Uuid::new_v4(),
        admin_account_id: unique_checksum_address(),
        post_id,
        token_id: token,
        action: ModerationAction::Restore,
        changed: false,
    };
    let audit = matching_audit(&context);

    let result = service
        .finish_cms_unknown_after_lookup_with_bump(
            context,
            "connection lost after COMMIT".into(),
            Ok(Some(audit)),
            async { Err(anyhow::anyhow!("injected ranking failure")) },
        )
        .await;
    assert!(result.is_ok(), "confirmed DB result must survive Redis bump failure");
    assert_public_caches_cleared(&redis, &seeded, false).await;
}
```

- [ ] **Step 4: Run the cache/outcome conformance suite serially**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::author_and_cms -- --nocapture --test-threads=1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::unknown_commit -- --nocapture --test-threads=1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests::confirmed_cms_completion -- --nocapture --test-threads=1
```

Expected: all three commands pass in independent UUID-prefixed Redis processes. The confirmed exact-audit path returns `Ok(())` despite the injected ranking failure, all six payload keys are gone, and the isolated generation value is unchanged because that one bump failed.

- [ ] **Step 5: Commit**

```bash
git add src/services/dev_post/moderation_tests.rs
git commit -m "test(dev-post): cover moderation cache invalidation"
```

### Task 10: Add authenticated CMS routes with exact status and empty-body contracts

**Files:**
- Modify: `src/router/cms/path.rs`
- Modify: `src/router/cms/handler.rs`
- Modify: `src/router/cms/mod.rs`

**Interfaces:**
- Consumes: `DevPostService::{delete_post_as_admin, restore_post_as_admin}`.
- Produces runtime paths `/cms/dev-post/:post_id` and `/cms/dev-post/:post_id/restore`, docs paths with `{post_id}`, and handlers returning `AppResult<StatusCode>`.

- [ ] **Step 1: Write path and real-router RED tests**

Add `DeleteDevPost` and `RestoreDevPost` path tests to `src/router/cms/path.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderation_paths_separate_runtime_and_openapi_syntax() {
        assert_eq!(CmsPath::DeleteDevPost.as_str(), "/cms/dev-post/:post_id");
        assert_eq!(
            CmsPath::DeleteDevPost.docs_str(),
            "/cms/dev-post/{post_id}"
        );
        assert_eq!(
            CmsPath::RestoreDevPost.as_str(),
            "/cms/dev-post/:post_id/restore"
        );
        assert_eq!(
            CmsPath::RestoreDevPost.docs_str(),
            "/cms/dev-post/{post_id}/restore"
        );
    }
}
```

Add this CMS router test scaffold in `src/router/cms/mod.rs`, then place the request assertions below inside `cms_moderation_routes_enforce_auth_status_and_empty_204`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{postgres::PostgresDatabase, r2::R2Client, redis::RedisDatabase},
        services::capricorn::CapricornClient,
    };
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use std::sync::Arc;
    use tower::ServiceExt;
    use tower_cookies::CookieManagerLayer;

    const ADMIN: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
    const NON_ADMIN: &str = "0xde709f2102306220921060314715629080e2fb77";
    const TOKEN: &str = "0x0000000000000000000000000000000000007010";

    #[sqlx::test(migrations = "./migrations-test")]
    async fn cms_moderation_routes_enforce_auth_status_and_empty_204(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let raw_prefix = std::env::var("REDIS_KEY_PREFIX")
            .expect("router Redis tests require an explicit REDIS_KEY_PREFIX");
        let prefix_uuid = raw_prefix
            .trim_end_matches(':')
            .strip_prefix("test-cms-dev-post-moderation-")
            .expect("refusing to run router tests outside the CMS moderation namespace");
        uuid::Uuid::parse_str(prefix_uuid)
            .expect("router REDIS_KEY_PREFIX must end in a per-process UUID");
        assert!(!crate::config::REDIS_KEY_PREFIX.is_empty());
        sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
            .bind(ADMIN)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
             VALUES ($1, 'CMS', 'CMS', 'img', $2, 0, '0x7010', 0)",
        )
        .bind(TOKEN)
        .bind(ADMIN)
        .execute(&pool)
        .await
        .unwrap();
        let post_id: i64 = sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'cms') RETURNING id",
        )
        .bind(TOKEN)
        .bind(ADMIN)
        .fetch_one(&pool)
        .await
        .unwrap();
        let redis = Arc::new(RedisDatabase::new().await);
        let admin_session = uuid::Uuid::new_v4().simple().to_string();
        let non_admin_session = uuid::Uuid::new_v4().simple().to_string();
        redis.set_session(&admin_session, ADMIN, 60_000).await.unwrap();
        redis
            .set_session(&non_admin_session, NON_ADMIN, 60_000)
            .await
            .unwrap();
        let state = AppState {
            postgres: Arc::new(PostgresDatabase {
                write_pool: pool.clone(),
                read_pool: pool.clone(),
            }),
            redis: redis.clone(),
            r2: Arc::new(R2Client::new().await),
            capricorn: Arc::new(CapricornClient::new(String::new())),
        };
        let app = router(state.clone())
            .layer(CookieManagerLayer::new())
            .with_state(state);
        let cookie_name =
            std::env::var("COOKIE_NAME").expect("COOKIE_NAME must be exported before cargo test");
        let admin_cookie = format!("{cookie_name}={admin_session}");
        let non_admin_cookie = format!("{cookie_name}={non_admin_session}");

        redis.delete_session(&admin_session).await.unwrap();
        redis.delete_session(&non_admin_session).await.unwrap();
        redis.delete_devpost_feed("global").await.unwrap();
        redis.delete_devpost_feed(TOKEN).await.unwrap();
        redis.delete_devpost_detail(post_id).await.unwrap();
        redis.delete_devpost_trending().await.unwrap();
    }
}
```

Place the following request/assertion block immediately before the first `redis.delete_session` cleanup statement:

```rust
let unauthenticated_malformed = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::DELETE)
            .uri("/cms/dev-post/not-an-i64")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(unauthenticated_malformed.status(), StatusCode::UNAUTHORIZED);

let invalid_session_malformed = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::DELETE)
            .uri("/cms/dev-post/not-an-i64")
            .header(
                header::COOKIE,
                format!("{cookie_name}={}", uuid::Uuid::new_v4().simple()),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(invalid_session_malformed.status(), StatusCode::UNAUTHORIZED);

let authenticated_malformed = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::DELETE)
            .uri("/cms/dev-post/not-an-i64")
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(authenticated_malformed.status(), StatusCode::BAD_REQUEST);

for uri in ["/cms/dev-post/0", "/cms/dev-post/-1"] {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(uri)
                .header(header::COOKIE, &admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

for uri in [format!("/cms/dev-post/{post_id}"), "/cms/dev-post/9223372036854775807".into()] {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(uri)
                .header(header::COOKIE, &non_admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

let deleted = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/cms/dev-post/{post_id}"))
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
assert_eq!(to_bytes(deleted.into_body(), usize::MAX).await.unwrap().len(), 0);

let repeated_delete = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/cms/dev-post/{post_id}"))
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(repeated_delete.status(), StatusCode::NO_CONTENT);

let restored = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/cms/dev-post/{post_id}/restore"))
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(restored.status(), StatusCode::NO_CONTENT);
assert_eq!(to_bytes(restored.into_body(), usize::MAX).await.unwrap().len(), 0);

let non_admin_restore_missing = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/cms/dev-post/9223372036854775807/restore")
            .header(header::COOKIE, &non_admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(non_admin_restore_missing.status(), StatusCode::FORBIDDEN);

let admin_restore_missing = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/cms/dev-post/9223372036854775807/restore")
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(admin_restore_missing.status(), StatusCode::NOT_FOUND);

sqlx::query("DELETE FROM token WHERE token_id = $1")
    .bind(TOKEN)
    .execute(&pool)
    .await
    .unwrap();
let orphan_restore = app
    .clone()
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/cms/dev-post/{post_id}/restore"))
            .header(header::COOKIE, &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(orphan_restore.status(), StatusCode::CONFLICT);
```

The test setup must seed two Redis sessions, only one matching an exact `admin.account_id`, one token/post, and clean up those exact sessions plus Dev Post cache keys at the end.

- [ ] **Step 2: Run RED**

```bash
set -a
source .env
set +a
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test router::cms -- --nocapture
```

Expected: path test compile failure because the variants do not exist; router requests are `404` until routes are registered.

- [ ] **Step 3: Implement path variants, handlers, and protected registration**

Extend `CmsPath` exactly:

```rust
DeleteDevPost,
RestoreDevPost,
```

Add match arms:

```rust
CmsPath::DeleteDevPost => "/cms/dev-post/:post_id",
CmsPath::RestoreDevPost => "/cms/dev-post/:post_id/restore",
```

Implement `docs_str` as a separate match so only the dynamic paths differ:

```rust
pub fn docs_str(&self) -> &'static str {
    match self {
        CmsPath::DeleteDevPost => "/cms/dev-post/{post_id}",
        CmsPath::RestoreDevPost => "/cms/dev-post/{post_id}/restore",
        _ => self.as_str(),
    }
}
```

Add handler imports for `Path`, `StatusCode`, `AppResult`, and `DevPostService`, then add:

```rust
#[utoipa::path(
    delete,
    path = CmsPath::DeleteDevPost.docs_str(),
    params(("post_id" = i64, Path, description = "Positive Dev Post BIGINT ID")),
    responses(
        (status = 204, description = "Post deleted or already deleted; empty response body"),
        (status = 400, description = "Malformed or non-positive post ID"),
        (status = 401, description = "Session required"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Physical post row not found"),
        (status = 500, description = "Internal error or outcome_unknown commit")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, session_address))]
pub async fn delete_dev_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .delete_post_as_admin(post_id, &session_address)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = CmsPath::RestoreDevPost.docs_str(),
    params(("post_id" = i64, Path, description = "Positive Dev Post BIGINT ID")),
    responses(
        (status = 204, description = "Post restored or already live; empty response body"),
        (status = 400, description = "Malformed or non-positive post ID"),
        (status = 401, description = "Session required"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "Physical post row not found"),
        (status = 409, description = "The post token no longer exists"),
        (status = 500, description = "Internal error or outcome_unknown commit")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, session_address))]
pub async fn restore_dev_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .restore_post_as_admin(post_id, &session_address)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
```

Register the two routes in `router` with middleware on each route:

```rust
.route(
    CmsPath::DeleteDevPost.as_str(),
    axum::routing::delete(handler::delete_dev_post)
        .layer(from_fn_with_state(state.clone(), authenticate_user)),
)
.route(
    CmsPath::RestoreDevPost.as_str(),
    post(handler::restore_dev_post)
        .layer(from_fn_with_state(state.clone(), authenticate_user)),
)
```

- [ ] **Step 4: Run the CMS router contract in GREEN**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test router::cms -- --nocapture
```

Expected: unauthenticated malformed path is `401`; authenticated malformed/non-positive are `400`; non-admin existing/missing are both `403`; delete/restore success bodies are zero bytes.

- [ ] **Step 5: Verify router formatting and whitespace**

```bash
cargo fmt --all -- --check
git diff --check
```

Expected: both commands exit `0`.

- [ ] **Step 6: Commit**

```bash
git add src/router/cms/path.rs src/router/cms/handler.rs src/router/cms/mod.rs
git commit -m "feat(cms): add Dev Post delete and restore routes"
```

### Task 11: Prove rollback at each pre-commit mutation stage

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: final controller transaction entry points.
- Produces: isolated trigger-driven proof that pin cleanup, post transition, restore, and audit insertion roll back atomically.

- [ ] **Step 1: Add conformance rollback tests for every pre-commit mutation stage**

Add this assertion helper:

```rust
async fn assert_live_pinned_without_audit(pool: &sqlx::PgPool, post_id: i64) {
    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM dev_post WHERE id = $1")
            .bind(post_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let pins: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(deleted_at.is_none());
    assert_eq!(pins, 1);
    assert_eq!(audits, 0);
}
```

Add this exact helper and three isolated `#[sqlx::test]` cases. Each test database gets one failure trigger, so trigger names never collide:

```rust
async fn assert_delete_stage_failure_rolls_back(pool: sqlx::PgPool, trigger_ddl: &str) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(TOKEN)
        .bind(post_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(trigger_ddl).execute(&pool).await.unwrap();
    assert!(matches!(
        controller(pool.clone())
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::InternalError(_))
    ));
    assert_live_pinned_without_audit(&pool, post_id).await;
}
```

Wrap those exact DDL strings in these tests:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn rollback_on_pin_cleanup_failure(pool: sqlx::PgPool) {
    assert_delete_stage_failure_rolls_back(
        pool,
        "CREATE FUNCTION fail_pin_cleanup() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected pin cleanup failure'; END $$; \
         CREATE TRIGGER fail_pin_cleanup BEFORE DELETE ON dev_post_pin \
         FOR EACH ROW EXECUTE FUNCTION fail_pin_cleanup()",
    )
    .await;
}

#[sqlx::test(migrations = "./migrations-test")]
async fn rollback_on_post_update_failure(pool: sqlx::PgPool) {
    assert_delete_stage_failure_rolls_back(
        pool,
        "CREATE FUNCTION fail_post_update() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected post update failure'; END $$; \
         CREATE TRIGGER fail_post_update BEFORE UPDATE ON dev_post \
         FOR EACH ROW EXECUTE FUNCTION fail_post_update()",
    )
    .await;
}

#[sqlx::test(migrations = "./migrations-test")]
async fn rollback_on_audit_insert_failure(pool: sqlx::PgPool) {
    assert_delete_stage_failure_rolls_back(
        pool,
        "CREATE FUNCTION fail_audit_insert() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected audit failure'; END $$; \
         CREATE TRIGGER fail_audit_insert BEFORE INSERT ON dev_post_moderation_log \
         FOR EACH ROW EXECUTE FUNCTION fail_audit_insert()",
    )
    .await;
}

#[sqlx::test(migrations = "./migrations-test")]
async fn rollback_restore_when_audit_insert_fails(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let controller = controller(pool.clone());
    committed(
        controller
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    sqlx::raw_sql(
        "CREATE FUNCTION fail_restore_audit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected restore audit failure'; END $$; \
         CREATE TRIGGER fail_restore_audit BEFORE INSERT ON dev_post_moderation_log \
         FOR EACH ROW EXECUTE FUNCTION fail_restore_audit()",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::InternalError(_))
    ));
    let still_deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(still_deleted);
    assert_eq!(audits, 1);
}
```

For the audit failure, pin deletion and `deleted_at` update occur before the failing INSERT; the assertion proves explicit rollback restores both.

- [ ] **Step 2: Run the rollback conformance tests**

```bash
cargo test controllers::dev_post::moderation_tests::rollback_ -- --nocapture
```

Expected: all four tests pass. They are GREEN conformance tests because Task 7 already owns the transaction implementation; a failure means the Task 7 transaction boundary is incomplete.

- [ ] **Step 3: Commit only the rollback tests**

```bash
git add src/controllers/dev_post/moderation_tests.rs
git diff --cached --check
git commit -m "test(dev-post): prove moderation rollback boundaries"
```

### Task 12: Prove relationship preservation and conditional pin restore

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: Task 7 controller transaction entry points and Task 11 rollback coverage.
- Produces: exact no-race relationship snapshots, historical-pin removal, live-pin preservation, and a fresh-poll non-snapshot race contract covering a new like, a separate pre-existing like removal, edit, and vote.

- [ ] **Step 1: Add the exact relationship and conditional-pin seed**

Add this complete seed helper. It creates the like and vote explicitly for the preservation test. The race seed below starts without a vote or `OTHER` like, but deliberately inserts a distinct pre-existing like for `UNLIKE_ACCOUNT` so concurrent like and unlike outcomes cannot be conflated.

```rust
async fn seed_relationship_post(pool: &sqlx::PgPool) -> i64 {
    let post_id = seed_token_and_post(pool).await;
    sqlx::query(
        "UPDATE dev_post SET created_at = TIMESTAMPTZ '2025-01-01 00:00:00Z', \
         updated_at = TIMESTAMPTZ '2025-01-02 00:00:00Z', \
         edited_at = TIMESTAMPTZ '2025-01-02 00:00:00Z' WHERE id = $1",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dev_post_image (post_id, position, image_uri) VALUES ($1, 0, 'image')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dev_post_poll (post_id, closes_at) \
         VALUES ($1, TIMESTAMPTZ '2025-01-03 00:00:00Z')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dev_post_poll_option (post_id, position, label) \
         VALUES ($1, 1, 'yes'), ($1, 2, 'no')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dev_post_like (post_id, account_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(OTHER)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO dev_post_poll_vote (post_id, account_id, option_position) \
         VALUES ($1, $2, 1)",
    )
    .bind(post_id)
    .bind(OTHER)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(TOKEN)
        .bind(post_id)
        .execute(pool)
        .await
        .unwrap();
    post_id
}

const UNLIKE_ACCOUNT: &str = "0x27b1fdb04752bbc536007a920d24acb045561c26";

async fn seed_fresh_live_poll_with_separate_unlike_target(pool: &sqlx::PgPool) -> i64 {
    let post_id = seed_token_and_post(pool).await;
    sqlx::query(
        "INSERT INTO dev_post_poll (post_id, closes_at) \
         VALUES ($1, clock_timestamp() + INTERVAL '1 day')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dev_post_poll_option (post_id, position, label) \
         VALUES ($1, 1, 'one'), ($1, 2, 'two')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dev_post_like (post_id, account_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(UNLIKE_ACCOUNT)
        .execute(pool)
        .await
        .unwrap();
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT count(*) FROM dev_post_like WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll_vote WHERE post_id = $1)",
    )
    .bind(post_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 0));
    post_id
}
```

- [ ] **Step 2: Add the relationship and conditional-pin conformance test**

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn restore_preserves_relationships_and_applies_conditional_pin_rule(pool: sqlx::PgPool) {
    let post_id = seed_relationship_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let post_times: (
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT created_at, updated_at, edited_at FROM dev_post WHERE id = $1",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let closes_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT closes_at FROM dev_post_poll WHERE post_id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let relation_counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT count(*) FROM dev_post_image WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_like WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll_option WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll_vote WHERE post_id = $1)",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(relation_counts, (1, 1, 1, 2, 1));
    let controller = controller(pool.clone());
    assert!(
        committed(
            controller
                .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
                .await
                .unwrap(),
        )
        .changed
    );
    assert!(
        committed(
            controller
                .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
                .await
                .unwrap(),
        )
        .changed
    );
    let after_counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT count(*) FROM dev_post_image WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_like WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll_option WHERE post_id = $1), \
           (SELECT count(*) FROM dev_post_poll_vote WHERE post_id = $1)",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let after_times: (
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT created_at, updated_at, edited_at FROM dev_post WHERE id = $1",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let after_closes_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT closes_at FROM dev_post_poll WHERE post_id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pin_count_after_restore: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_counts, relation_counts);
    assert_eq!(after_times, post_times);
    assert_eq!(after_closes_at, closes_at);
    assert_eq!(pin_count_after_restore, 0);
    let restored = controller.get_post_base(post_id).await.unwrap();
    assert!(restored.poll.as_ref().unwrap().is_closed);
    let feed = controller.get_feed_base(Some(TOKEN), 1, 10).await.unwrap();
    assert!(feed.pin.is_none());
    assert!(feed.posts.iter().any(|post| post.id == post_id.to_string()));

    controller.pin_post(post_id, ADMIN).await.unwrap();
    let live_restore = committed(
        controller
            .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    let pin_count_after_live_restore: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!live_restore.changed);
    assert_eq!(pin_count_after_live_restore, 1);
}
```

- [ ] **Step 3: Add the fresh-poll non-snapshot race conformance test**

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn restore_race_preserves_each_existing_mutation_result_without_snapshot_promise(
    pool: sqlx::PgPool,
) {
    let post_id = seed_fresh_live_poll_with_separate_unlike_target(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let initial_body: String = sqlx::query_scalar("SELECT body FROM dev_post WHERE id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let delete_controller = controller(pool.clone());
    committed(
        delete_controller
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
            .unwrap(),
    );
    let restore_controller = controller(pool.clone());
    let edit_controller = controller(pool.clone());
    let like_controller = controller(pool.clone());
    let unlike_controller = controller(pool.clone());
    let vote_controller = controller(pool.clone());
    let (restore, edit, like, unlike, vote) = tokio::join!(
        restore_controller.restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4()),
        edit_controller.edit_post(
            post_id,
            ADMIN,
            &EditDevPostRequest {
                body: Some("raced edit".into()),
                image_uris: None,
            },
        ),
        like_controller.like(post_id, OTHER),
        unlike_controller.unlike(post_id, UNLIKE_ACCOUNT),
        vote_controller.vote(post_id, OTHER, 1),
    );
    assert!(restore.is_ok());
    assert!(edit.is_ok() || matches!(&edit, Err(AppError::NotFound(_))));
    assert!(like.is_ok() || matches!(&like, Err(AppError::NotFound(_))));
    assert!(unlike.is_ok());
    assert!(vote.is_ok() || matches!(&vote, Err(AppError::NotFound(_))));
    let body: String = sqlx::query_scalar("SELECT body FROM dev_post WHERE id = $1")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let like_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dev_post_like WHERE post_id = $1 AND account_id = $2)",
    )
    .bind(post_id)
    .bind(OTHER)
    .fetch_one(&pool)
    .await
    .unwrap();
    let unlike_target_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dev_post_like WHERE post_id = $1 AND account_id = $2)",
    )
    .bind(post_id)
    .bind(UNLIKE_ACCOUNT)
    .fetch_one(&pool)
    .await
    .unwrap();
    let vote_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dev_post_poll_vote WHERE post_id = $1 AND account_id = $2)",
    )
    .bind(post_id)
    .bind(OTHER)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(body, if edit.is_ok() { "raced edit" } else { &initial_body });
    assert_eq!(like_exists, like.is_ok());
    assert!(!unlike_target_exists, "restore must not resurrect the independently removed like");
    assert_eq!(vote_exists, vote.is_ok());
}
```

- [ ] **Step 4: Run the relationship/pin conformance tests**

```bash
cargo test controllers::dev_post::moderation_tests::restore_preserves_relationships -- --nocapture
cargo test controllers::dev_post::moderation_tests::restore_race_preserves -- --nocapture
```

Expected: both pass; production moderation SQL contains no write to relationship tables, actual restore removes a historical pin, live no-op restore preserves a current pin, and restore never resurrects the separate like successfully removed by the concurrent unlike.

- [ ] **Step 5: Commit only the relationship and pin tests**

```bash
git add src/controllers/dev_post/moderation_tests.rs
git diff --cached --check
git commit -m "test(dev-post): preserve moderation relationships and pins"
```

### Task 13: Prove same-primary serialization and admin-revocation locking

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: Task 7 controller moderation methods and existing `pin_post`.
- Produces: exact same-primary `changed` winner counts, deterministic final state derived from returned contexts, exact audit UUID/changed agreement without timestamp or random-UUID ordering, pin race safety, and controller-held admin lock evidence.

- [ ] **Step 1: Add exact concurrent DELETE and RESTORE conformance tests**

```rust
async fn assert_exact_audit_matches_context(
    pool: &sqlx::PgPool,
    context: &ModerationContext,
) {
    let row: (String, bool) = sqlx::query_as(
        "SELECT action, changed FROM dev_post_moderation_log WHERE id = $1",
    )
    .bind(context.audit_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row, (context.action.as_str().to_string(), context.changed));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn concurrent_same_primary_deletes_and_restores_have_one_local_change(
    pool: sqlx::PgPool,
) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let delete_left_id = uuid::Uuid::from_u128(0x1301);
    let delete_right_id = uuid::Uuid::from_u128(0x1302);
    let (left, right) = tokio::join!(
        controller(pool.clone()).delete_post_as_admin(post_id, ADMIN, delete_left_id),
        controller(pool.clone()).delete_post_as_admin(post_id, ADMIN, delete_right_id),
    );
    let delete_left = committed(left.unwrap());
    let delete_right = committed(right.unwrap());
    assert_eq!(delete_left.audit_id, delete_left_id);
    assert_eq!(delete_right.audit_id, delete_right_id);
    let mut delete_changed = vec![delete_left.changed, delete_right.changed];
    delete_changed.sort_unstable();
    assert_eq!(delete_changed, vec![false, true]);
    assert_exact_audit_matches_context(&pool, &delete_left).await;
    assert_exact_audit_matches_context(&pool, &delete_right).await;

    let restore_left_id = uuid::Uuid::from_u128(0x1303);
    let restore_right_id = uuid::Uuid::from_u128(0x1304);
    let (left, right) = tokio::join!(
        controller(pool.clone()).restore_post_as_admin(post_id, ADMIN, restore_left_id),
        controller(pool.clone()).restore_post_as_admin(post_id, ADMIN, restore_right_id),
    );
    let restore_left = committed(left.unwrap());
    let restore_right = committed(right.unwrap());
    assert_eq!(restore_left.audit_id, restore_left_id);
    assert_eq!(restore_right.audit_id, restore_right_id);
    let mut restore_changed = vec![restore_left.changed, restore_right.changed];
    restore_changed.sort_unstable();
    assert_eq!(restore_changed, vec![false, true]);
    assert_exact_audit_matches_context(&pool, &restore_left).await;
    assert_exact_audit_matches_context(&pool, &restore_right).await;
}

#[sqlx::test(migrations = "./migrations-test")]
async fn concurrent_same_primary_delete_restore_final_state_matches_returned_contexts(
    pool: sqlx::PgPool,
) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let delete_id = uuid::Uuid::from_u128(0x1305);
    let restore_id = uuid::Uuid::from_u128(0x1306);
    let (delete, restore) = tokio::join!(
        controller(pool.clone()).delete_post_as_admin(post_id, ADMIN, delete_id),
        controller(pool.clone()).restore_post_as_admin(post_id, ADMIN, restore_id),
    );
    let delete = committed(delete.unwrap());
    let restore = committed(restore.unwrap());
    assert_eq!(delete.audit_id, delete_id);
    assert_eq!(restore.audit_id, restore_id);
    assert!(delete.changed, "a live post must be deleted exactly once");
    assert_exact_audit_matches_context(&pool, &delete).await;
    assert_exact_audit_matches_context(&pool, &restore).await;
    let deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        deleted,
        !restore.changed,
        "restore changed=true means DELETE serialized first and final state is live; \
         restore changed=false means RESTORE serialized first and DELETE left it deleted",
    );
}
```

- [ ] **Step 2: Run the same-primary state tests**

```bash
cargo test controllers::dev_post::moderation_tests::concurrent_same_primary_ -- --nocapture
```

Expected: both tests pass. Every audit is read by its supplied UUID and matches its returned `changed` context exactly; final state is inferred from those contexts, with no `ORDER BY created_at`, UUID ordering, or random UUIDs used as a serialization oracle.

- [ ] **Step 3: Add exact pin/DELETE and live-RESTORE/pin tests**

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn pin_delete_race_cannot_leave_a_deleted_post_pinned(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let (pin_result, delete_result) = tokio::join!(
        controller(pool.clone()).pin_post(post_id, ADMIN),
        controller(pool.clone()).delete_post_as_admin(
            post_id,
            ADMIN,
            uuid::Uuid::new_v4(),
        ),
    );
    assert!(delete_result.is_ok());
    assert!(pin_result.is_ok() || matches!(pin_result, Err(AppError::NotFound(_))));
    let state: (bool, i64) = sqlx::query_as(
        "SELECT deleted_at IS NOT NULL, \
           (SELECT count(*) FROM dev_post_pin WHERE post_id = $1) \
         FROM dev_post WHERE id = $1",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, (true, 0));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn live_restore_pin_race_finishes_without_deadlock_and_keeps_pin(pool: sqlx::PgPool) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let joined = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(
            controller(pool.clone()).restore_post_as_admin(
                post_id,
                ADMIN,
                uuid::Uuid::new_v4(),
            ),
            controller(pool.clone()).pin_post(post_id, ADMIN),
        )
    })
    .await
    .expect("live RESTORE/pin deadlocked");
    assert!(joined.0.is_ok());
    assert!(joined.1.is_ok());
    let pin_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pin_count, 1);
}
```

- [ ] **Step 4: Run the pin race tests**

```bash
cargo test controllers::dev_post::moderation_tests::pin_delete_race -- --nocapture
cargo test controllers::dev_post::moderation_tests::live_restore_pin_race -- --nocapture
```

Expected: both pass within two seconds; the deleted state has no pin and the live state retains one.

- [ ] **Step 5: Add the controller moderation/admin-revocation lock test**

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn controller_moderation_holds_admin_lock_until_commit_then_revocation_wins(
    pool: sqlx::PgPool,
) {
    let post_id = seed_token_and_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    let mut post_blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM dev_post WHERE id = $1 FOR UPDATE")
        .bind(post_id)
        .execute(&mut *post_blocker)
        .await
        .unwrap();
    let moderation_pool = pool.clone();
    let moderation = tokio::spawn(async move {
        controller(moderation_pool)
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let waiting_on_post: bool = sqlx::query_scalar(
                "SELECT EXISTS( \
                   SELECT 1 FROM pg_stat_activity \
                   WHERE datname = current_database() \
                     AND wait_event_type = 'Lock' \
                     AND query LIKE 'SELECT (deleted_at IS NOT NULL)%FOR UPDATE%')",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if waiting_on_post {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("moderation never reached the post lock after locking admin");
    let revoke_pool = pool.clone();
    let mut revoke = tokio::spawn(async move {
        sqlx::query("DELETE FROM admin WHERE account_id = $1")
            .bind(ADMIN)
            .execute(&revoke_pool)
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut revoke)
            .await
            .is_err(),
        "admin revocation bypassed controller-held FOR KEY SHARE",
    );
    post_blocker.commit().await.unwrap();
    committed(moderation.await.unwrap().unwrap());
    revoke.await.unwrap().unwrap();
    assert!(matches!(
        controller(pool)
            .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4())
            .await,
        Err(AppError::Forbidden(_))
    ));
}
```

- [ ] **Step 6: Run the controller-held admin lock test**

```bash
cargo test controllers::dev_post::moderation_tests::controller_moderation_holds_admin_lock -- --nocapture
```

Expected: revocation is blocked while the real moderation controller holds the admin lock, then completes after moderation commits; a later moderation call is `403`.

- [ ] **Step 7: Commit only same-primary concurrency tests**

```bash
git add src/controllers/dev_post/moderation_tests.rs
git diff --cached --check
git commit -m "test(dev-post): prove same-primary moderation serialization"
```

### Task 14: Qualify pgactive cross-writer convergence and retry semantics

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes: final controller moderation and feed selection behavior.
- Produces: an opt-in, convergence-compatible two-writer test that makes no global winner or ordering claim.

- [ ] **Step 1: Add the opt-in pgactive qualification test**

Add this opt-in test in the same module. It creates unique EIP-55 values, waits for visibility/convergence with bounded exact queries, never logs either URL, and deliberately does not assert a global winner:

```rust
#[tokio::test]
async fn pgactive_cross_writer_moderation_converges_and_retry_is_safe() {
    let (Ok(url_a), Ok(url_b)) = (
        std::env::var("PGACTIVE_WRITER_A_URL"),
        std::env::var("PGACTIVE_WRITER_B_URL"),
    ) else {
        eprintln!(
            "pgactive qualification skipped: set PGACTIVE_WRITER_A_URL and PGACTIVE_WRITER_B_URL"
        );
        return;
    };
    let pool_a = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url_a)
        .await
        .unwrap();
    let pool_b = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url_b)
        .await
        .unwrap();
    let seed = uuid::Uuid::new_v4().simple().to_string();
    let admin = crate::utils::valid_account_id(&format!("0x{}00000001", &seed[..32])).unwrap();
    let token = crate::utils::valid_account_id(&format!("0x{}00000002", &seed[..32])).unwrap();
    sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
        .bind(&admin)
        .execute(&pool_a)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'PGActive', 'PGA', 'img', $2, 0, $3, 0)",
    )
    .bind(&token)
    .bind(&admin)
    .bind(format!("0x{seed}"))
    .execute(&pool_a)
    .await
    .unwrap();
    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'pgactive') RETURNING id",
    )
    .bind(&token)
    .bind(&admin)
    .fetch_one(&pool_a)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(&token)
        .bind(post_id)
        .execute(&pool_a)
        .await
        .unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let visible: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM admin WHERE account_id = $1) \
                 AND EXISTS(SELECT 1 FROM token WHERE token_id = $2) \
                 AND EXISTS(SELECT 1 FROM dev_post WHERE id = $3 AND token_id = $2) \
                 AND EXISTS(SELECT 1 FROM dev_post_pin WHERE token_id = $2 AND post_id = $3)",
            )
            .bind(&admin)
            .bind(&token)
            .bind(post_id)
            .fetch_one(&pool_b)
            .await
            .unwrap();
            if visible {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("admin/token/post/pin seed did not replicate to writer B");

    let delete_audit = uuid::Uuid::new_v4();
    let restore_audit = uuid::Uuid::new_v4();
    let controller_a = controller(pool_a.clone());
    let controller_b = controller(pool_b.clone());
    let (delete_result, restore_result) = tokio::join!(
        controller_a.delete_post_as_admin(post_id, &admin, delete_audit),
        controller_b.restore_post_as_admin(post_id, &admin, restore_audit),
    );
    assert!(delete_result.is_ok());
    assert!(restore_result.is_ok());

    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let state_a: bool = sqlx::query_scalar(
                "SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1",
            )
            .bind(post_id)
            .fetch_one(&pool_a)
            .await
            .unwrap();
            let state_b: bool = sqlx::query_scalar(
                "SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1",
            )
            .bind(post_id)
            .fetch_one(&pool_b)
            .await
            .unwrap();
            let audit_count_a: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM dev_post_moderation_log WHERE id = ANY($1)",
            )
            .bind(vec![delete_audit, restore_audit])
            .fetch_one(&pool_a)
            .await
            .unwrap();
            let audit_count_b: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM dev_post_moderation_log WHERE id = ANY($1)",
            )
            .bind(vec![delete_audit, restore_audit])
            .fetch_one(&pool_b)
            .await
            .unwrap();
            if state_a == state_b && audit_count_a == 2 && audit_count_b == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("concurrent moderation did not converge");

    let rows: Vec<(uuid::Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, action, changed FROM dev_post_moderation_log \
         WHERE id = ANY($1) ORDER BY id",
    )
    .bind(vec![delete_audit, restore_audit])
    .fetch_all(&pool_a)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].0, rows[1].0);
    assert!(rows.iter().all(|(_, action, _)| action == "DELETE" || action == "RESTORE"));

    let retry_audit = uuid::Uuid::new_v4();
    assert!(
        controller_a
            .delete_post_as_admin(post_id, &admin, retry_audit)
            .await
            .is_ok()
    );
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let converged: bool = sqlx::query_scalar(
                "SELECT \
                   (SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1) \
                   AND NOT EXISTS(SELECT 1 FROM dev_post_pin WHERE post_id = $1)",
            )
            .bind(post_id)
            .fetch_one(&pool_a)
            .await
            .unwrap();
            let converged_b: bool = sqlx::query_scalar(
                "SELECT \
                   (SELECT deleted_at IS NOT NULL FROM dev_post WHERE id = $1) \
                   AND NOT EXISTS(SELECT 1 FROM dev_post_pin WHERE post_id = $1)",
            )
            .bind(post_id)
            .fetch_one(&pool_b)
            .await
            .unwrap();
            if converged && converged_b {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("idempotent DELETE retry did not converge to deleted/unpinned");

    for pool in [&pool_a, &pool_b] {
        let feed = controller(pool.clone())
            .get_feed_base(Some(&token), 1, 10)
            .await
            .unwrap();
        assert!(feed.pin.is_none());
        assert!(feed.posts.iter().all(|post| post.id != post_id.to_string()));
    }
}
```

- [ ] **Step 2: Run the pgactive qualification test against both writers**

```bash
cargo test pgactive_cross_writer_moderation_converges_and_retry_is_safe -- --nocapture --test-threads=1
```

Expected: pass when both writer URLs point to migrated test writers; without those variables, the test reports that pgactive qualification was skipped and does not make a false serialization claim.

- [ ] **Step 3: Run the complete controller moderation suite**

```bash
cargo test controllers::dev_post::moderation_tests -- --nocapture
```

Expected: all same-primary tests pass; the opt-in pgactive test either passes against configured writers or emits only the documented skip message.

- [ ] **Step 4: Verify controller test formatting**

```bash
cargo fmt --all -- --check
```

Expected: exit `0`.

- [ ] **Step 5: Commit only the pgactive qualification test**

```bash
git add src/controllers/dev_post/moderation_tests.rs
git commit -m "test(dev-post): qualify moderation transactions and races"
```

### Task 15: Register OpenAPI and publish the API/operations contract

**Files:**
- Modify: `src/main.rs`
- Create: `docs/cms-dev-post-moderation.md`
- Modify: `docs/dev-post-api.md`
- Modify: `docs/V2_API_CHANGES.md`

**Interfaces:**
- Consumes: final CMS handlers and implemented cache/transaction semantics.
- Produces: Swagger UI, OpenAPI JSON, Scalar, CMS operational reference, corrected Dev Post cache docs, and endpoint-count assertions.

- [ ] **Step 1: Add RED OpenAPI and documentation contract tests**

In `src/main.rs` `openapi_tests`, add:

```rust
#[test]
fn cms_dev_post_moderation_paths_and_empty_responses_are_documented() {
    let json = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let delete = &json["paths"]["/cms/dev-post/{post_id}"]["delete"];
    let restore = &json["paths"]["/cms/dev-post/{post_id}/restore"]["post"];
    assert!(delete.is_object());
    assert!(restore.is_object());
    assert!(delete["requestBody"].is_null());
    assert!(restore["requestBody"].is_null());
    for status in ["204", "400", "401", "403", "404", "500"] {
        assert!(delete["responses"][status].is_object(), "DELETE missing {status}");
        assert!(restore["responses"][status].is_object(), "RESTORE missing {status}");
    }
    assert!(restore["responses"]["409"].is_object());
    assert!(delete["responses"]["204"]["content"].is_null());
    assert!(restore["responses"]["204"]["content"].is_null());

    let paths = json["paths"].as_object().unwrap();
    let methods = ["get", "post", "put", "patch", "delete"];
    let dev_post_operations: usize = paths
        .iter()
        .filter(|(path, _)| path.starts_with("/dev-post"))
        .map(|(_, item)| methods.iter().filter(|method| item[**method].is_object()).count())
        .sum();
    let cms_moderation_operations: usize = [
        "/cms/dev-post/{post_id}",
        "/cms/dev-post/{post_id}/restore",
    ]
    .iter()
    .map(|path| methods.iter().filter(|method| paths[*path][**method].is_object()).count())
    .sum();
    assert_eq!(dev_post_operations, 13);
    assert_eq!(cms_moderation_operations, 2);
}

#[test]
fn cms_moderation_docs_cover_audit_cache_rollout_and_endpoint_counts() {
    let cms = include_str!("../docs/cms-dev-post-moderation.md");
    for required in [
        "DELETE /cms/dev-post/{post_id}",
        "POST /cms/dev-post/{post_id}/restore",
        "changed=false",
        "outcome_unknown",
        "devpost:ranking:generation",
        "FOR KEY SHARE",
        "pgactive",
        "CMS controls",
        "빈 `204 No Content`",
        "`KEYS`, `SCAN`, `FLUSHALL`은 사용하지 않는다",
        "작성자 DELETE와 CMS DELETE 모두 active pin을 같은 transaction에서 제거",
        "실제 RESTORE는 stale historical pin을 제거",
        "live no-op RESTORE는 현재 pin을 유지",
    ] {
        assert!(cms.contains(required), "missing CMS moderation contract: {required}");
    }

    let dev_post = include_str!("../docs/dev-post-api.md");
    for required in [
        "Trending과 Ranking도 Redis cache-aside를 사용",
        "generation-scoped v2 key",
        "작성자 DELETE는 반복 요청이 404",
        "CMS DELETE/RESTORE는 반복 요청도 204",
        "복구된 글은 일반 게시물",
        "작성자 DELETE와 CMS DELETE 모두 active pin을 같은 transaction에서 제거",
        "실제 RESTORE는 stale historical pin을 제거",
        "live no-op RESTORE는 현재 pin을 유지",
        "dev_post_moderation_log",
        "changed=false",
        "old node drain",
    ] {
        assert!(dev_post.contains(required), "missing Dev Post reference: {required}");
    }
    assert!(!dev_post.contains("Trending / Ranking은 아직 Redis 캐싱되지 않습니다."));
    assert!(!dev_post.contains("현재는 매 요청마다 라이브 쿼리로 집계합니다"));
    assert!(!dev_post.contains("Moderation DELETE만 active pin"));

    let changes = include_str!("../docs/V2_API_CHANGES.md");
    assert!(changes.contains("Dev Post 13개"));
    assert!(changes.contains("CMS moderation 2개"));
    assert!(changes.contains("| DELETE | `/cms/dev-post/{post_id}`"));
    assert!(changes.contains("| POST | `/cms/dev-post/{post_id}/restore`"));
    assert!(!changes.contains("| Trending/Ranking Redis 캐싱 | 아직 미적용"));
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test openapi_tests::cms_ -- --nocapture
```

Expected: paths are null and the new focused document is absent.

- [ ] **Step 3: Register both handlers in utoipa**

In the CMS section of `ApiDoc`'s `paths(...)`, add:

```rust
router::cms::handler::delete_dev_post,
router::cms::handler::restore_dev_post,
```

No request schema is added because neither handler has a body. Existing `.merge(cms::router(app_state.clone()))`, Swagger UI `/dev-sw`, OpenAPI JSON `/dev-sw/openapi.json`, and Scalar `/dev-scalar` wiring remain unchanged.

- [ ] **Step 4: Create the exact CMS moderation operations document**

Create `docs/cms-dev-post-moderation.md` with these sections and contracts:

```markdown
# CMS Dev Post Moderation

## API

- `DELETE /cms/dev-post/{post_id}`: live post를 soft delete하고 active pin을 같은 transaction에서 제거한다.
- `POST /cms/dev-post/{post_id}/restore`: deleted post의 `deleted_at`만 `NULL`로 되돌린다.

두 API는 body를 받지 않으며 성공 시 빈 `204 No Content`를 반환한다. 실제 post row가 있으면 반복
DELETE/RESTORE도 204이고 각 요청은 `changed=false` audit row를 남긴다. 잘못된/0 이하 ID는 400,
세션 없음은 401, non-admin은 post 존재 여부와 무관하게 403, physical row 없음은 404, token이 없는
post RESTORE는 409다.

기존 작성자 DELETE와 CMS DELETE 모두 active pin을 같은 transaction에서 제거한다. CMS RESTORE는
삭제 권한이나 pin 권한을 가장하지 않고 아래 restore 규칙만 적용한다.

## 권한과 lock

세션 주소는 EIP-55 exact equality로 `admin.account_id`와 비교한다. transaction의 첫 DB read는 write
pool의 admin row `FOR KEY SHARE`이며, 이후 token `FOR KEY SHARE`, post `FOR UPDATE` 순서로 lock한다.
`LOWER()` 비교는 사용하지 않는다. DELETE는 orphan post도 허용하지만 RESTORE는 token row가 필요하다.

## Audit

`dev_post_moderation_log`는 애플리케이션이 생성한 UUID, post/token/admin, `DELETE|RESTORE`,
`changed`, 실행 시각을 영구 저장한다. foreign key가 없으므로 대상 post/token/admin이 물리 삭제돼도
남는다. 상태 변경과 audit INSERT는 같은 transaction이다. audit 조회 API와 삭제 사유 body는 없다.

`changed`는 `deleted_at` 전이만 뜻한다. 반복 성공은 `changed=false`다. `COMMIT` 응답이 끊기면 같은
UUID를 write pool에서 조회해 정확히 일치할 때만 204로 reconcile한다. 확인할 수 없으면 cache
invalidation을 시도하고 `500 outcome_unknown`을 반환한다. 재시도는 안전하며 새 UUID audit attempt를 만든다.

## Restore 범위

실제 RESTORE는 stale historical pin을 제거하고 과거 pin을 복구하지 않은 채 일반 `id DESC` 위치로
되돌린다. live no-op RESTORE는 현재 pin을 유지한다. 이미지, like, poll, option, vote, body, author, post timestamps,
poll close time은 moderation SQL이 변경하지 않는다.

## Cache consistency

확정 commit과 outcome-unknown commit 모두 global/token feed의 legacy+v2 key, detail, trending을 각각
삭제하고 `devpost:ranking:generation`을 `INCR`한다. Ranking payload는
`devpost:ranking:v2:{generation}:{page}:{limit}`만 사용한다. Redis 실패는 확정된 DB 성공을 실패로
바꾸지 않으며 replica lag/TTL 동안 stale read가 가능하다. `KEYS`, `SCAN`, `FLUSHALL`은 사용하지 않는다.

## pgactive

동일 primary의 row lock은 요청을 직렬화하지만 pgactive writer 사이의 전역 순서나 winner를 보장하지
않는다. cross-writer audit의 `changed`와 timestamp를 글로벌 순서로 해석하지 않는다. 수렴 후 idempotent
retry로 원하는 상태를 다시 적용할 수 있다.

## Rollout / rollback

1. migration을 API보다 먼저 배포하고 table/check/index를 확인한다.
2. 새 API를 배포하되 mixed-version 동안 CMS controls를 끈다.
3. old node drain과 replica lag를 확인한다.
4. 정확히 `REDIS_KEY_PREFIX`가 붙은 `devpost:trending`을 `DEL`한다.
5. 정확히 prefix가 붙은 `devpost:ranking:generation`을 `INCR`한다.
6. CMS controls를 켜고 4xx/5xx, audit/reconciliation, outcome_unknown, lock, Redis warning을 모니터링한다.

rollback은 먼저 CMS controls를 끄고 API binary만 되돌린다. audit table/data는 삭제하지 않는다. old
binary의 legacy ranking stale은 `DEVPOST_RANKING_EXPIRATION`까지 허용한다. 재배포 때 drain/lag/
exact DEL/exact INCR 절차를 다시 수행한다.
```

- [ ] **Step 5: Replace the contradictory Dev Post cache text and add moderation semantics**

In `docs/dev-post-api.md`, delete the complete limitation bullet beginning with `**Trending / Ranking은 아직 Redis 캐싱되지 않습니다.**`; do not leave either `현재는 매 요청마다 라이브 쿼리로 집계합니다` or the claim that caching is follow-up work. Insert this exact section before `## TypeScript Interfaces`:

```markdown
## CMS moderation DELETE / RESTORE

- `DELETE /cms/dev-post/{post_id}`와 `POST /cms/dev-post/{post_id}/restore`는 admin 전용이며 body가 없고 성공 body도 없는 204다.
- 작성자 DELETE는 반복 요청이 404이고 CMS DELETE/RESTORE는 실제 row가 있으면 반복 요청도 204다. CMS 반복 요청도 `dev_post_moderation_log`에 `changed=false` audit를 남긴다.
- 실제 RESTORE는 stale historical pin을 제거하고 과거 pin을 복구하지 않으며, 복구된 글은 일반 게시물 `id DESC` 위치로 돌아온다. live no-op RESTORE는 현재 pin을 유지한다.
- Pin PUT/DELETE 권한과 token-feed-only invalidation은 바뀌지 않는다. 작성자 DELETE와 CMS DELETE 모두 active pin을 같은 transaction에서 제거한다.
- 이미지, like, poll, option, vote와 기존 post timestamp는 보존되고 poll 마감은 연장하지 않는다.
- Trending과 Ranking도 Redis cache-aside를 사용한다. Trending은 exact key TTL cache이고 moderation 시 삭제한다. Ranking은 generation-scoped v2 key를 사용하며 moderation 시 generation을 원자적으로 올린다.
- 작성자 DELETE와 CMS DELETE/RESTORE는 global/token legacy+v2 feed, detail, trending, ranking generation 전체를 best-effort 무효화한다. Redis 실패와 read replica lag 동안 TTL 범위의 stale read가 가능하다.
- Rollout은 migration 선배포, CMS controls off, old node drain, replica lag 확인, exact prefixed trending `DEL`, exact prefixed generation `INCR`, CMS controls on 순서다. Rollback/redeploy에도 같은 절차를 반복한다.
```

- [ ] **Step 6: Replace the V2 uncached row and add the two CMS operations**

In `docs/V2_API_CHANGES.md`, replace this exact stale row:

```markdown
| Trending/Ranking Redis 캐싱 | 아직 미적용 — 매 요청 라이브 쿼리 (성능 후속 작업) |
```

with:

```markdown
| Trending/Ranking Redis 캐싱 | Redis cache-aside 적용 — Trending exact TTL key, Ranking generation-scoped v2 key |
```

Then add this CMS subsection after the 13-operation Dev Post section:

```markdown
### CMS Dev Post moderation

| Method | Path | Auth | 동작 |
|---|---|---|---|
| DELETE | `/cms/dev-post/{post_id}` | O (admin) | soft delete + unpin + audit, 반복 204 |
| POST | `/cms/dev-post/{post_id}/restore` | O (admin) | 일반 post로 restore + audit, 반복 204 |

Endpoint count: Dev Post 13개, CMS moderation 2개.
```

Do not change the existing Dev Post count from 13 and do not list the two CMS routes under the Dev Post route count.

- [ ] **Step 7: Run the OpenAPI plus positive/negative documentation assertions in GREEN**

```bash
cargo test openapi_tests::cms_ -- --nocapture
cargo test openapi_tests::dev_post -- --nocapture
```

Expected: OpenAPI exposes exactly 13 `/dev-post*` operations plus exactly 2 CMS moderation operations; the old uncached claims are absent and all restore/pin/idempotency/audit/rollout strings are present.

- [ ] **Step 8: Verify OpenAPI/document formatting and whitespace**

```bash
cargo fmt --all -- --check
git diff --check
```

Expected: both commands exit `0`.

- [ ] **Step 9: Commit the OpenAPI/documentation contract**

```bash
git add src/main.rs docs/cms-dev-post-moderation.md docs/dev-post-api.md docs/V2_API_CHANGES.md
git commit -m "docs(cms): publish Dev Post moderation contract"
```

Expected: OpenAPI exposes both operations with empty 204s and declared errors; all document assertions pass.

### Task 16: Full verification, two-stage review, and Draft API PR

**Files:**
- Verify all changed files from Tasks 1–15
- Modify only files implicated by review findings

**Interfaces:**
- Consumes: complete implementation and migrations squash commit.
- Produces: reviewed Draft API PR based on `v2`; no automatic API merge.

- [ ] **Step 1: Verify formatting**

```bash
set -a
source .env
set +a
cargo fmt --all -- --check
```

Expected: exit `0` with no formatting diff.

- [ ] **Step 2: Verify all targets and features compile**

```bash
cargo check --all-targets --all-features
```

Expected: exit `0`.

- [ ] **Step 3: Run the full test matrix in one fresh isolated Redis namespace**

```bash
test -n "${REDIS_TEST_URL:-}" || {
  echo "REDIS_TEST_URL must point to a dedicated disposable non-production Redis" >&2
  exit 1
}
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test --all-targets --all-features -- --test-threads=1
```

Expected: every test passes with `0 failed`; the process-level UUID prefix is nonempty and asserted by Redis tests before any key access.

- [ ] **Step 4: Run Clippy across all targets and features**

```bash
cargo clippy --all-targets --all-features
```

Expected: exit `0`; any baseline warning was recorded in Task 1 and no changed file adds a warning.

- [ ] **Step 5: Verify patch whitespace**

```bash
git diff --check
```

Expected: silent exit `0`.

- [ ] **Step 6: Run source-only address/ranking/enumeration safety scans**

```bash
rg -n "LOWER\(|lower\(|ILIKE" src/controllers/dev_post/moderation.rs src/services/dev_post src/router/cms || true
rg -n "devpost:ranking:\{page\}|get_devpost_ranking_response\(|set_devpost_ranking_response\(" src || true
if git diff --unified=0 v2...HEAD -- src/controllers/dev_post/moderation.rs src/services/dev_post src/router/cms src/db/redis/mod.rs | rg '^\+.*(KEYS|SCAN|FLUSHALL)'; then exit 1; fi
```

Expected: no changed source introduces case-folded address SQL, legacy ranking access, or Redis enumeration/flush.

- [ ] **Step 7: Verify the operator prohibition separately from the source scan**

```bash
rg -n '`KEYS`, `SCAN`, `FLUSHALL`은 사용하지 않는다' docs/cms-dev-post-moderation.md
```

Expected: exactly the operator prohibition is printed from documentation; docs are not fed to the source-code prohibition scan.

- [ ] **Step 8: Verify migration diff, remote head, and parent gitlink identity**

```bash
git diff v2...HEAD -- migrations migrations-test tests/dev_post_moderation_migration.rs
git -C migrations fetch origin v2:v2
test "$(git -C migrations rev-parse HEAD)" = "$(git -C migrations rev-parse origin/v2)"
test "$(git rev-parse HEAD:migrations)" = "$(git -C migrations rev-parse HEAD)"
```

Expected: the expected gitlink/symlink/test diff is shown and submodule HEAD, `origin/v2`, and the API gitlink are identical.

- [ ] **Step 9: Run the focused service suite in a new process-isolated Redis namespace**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test services::dev_post::moderation_tests -- --nocapture --test-threads=1
```

Expected: all service tests pass; exact global/trending/generation keys are prefixed, the in-process mutex prevents overlap inside this process, and the fresh prefix isolates this run from every router or parallel Cargo process.

- [ ] **Step 10: Run the focused HTTP/controller/migration suites serially**

```bash
test -n "${REDIS_TEST_URL:-}" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-cms-dev-post-moderation-$(uuidgen | tr '[:upper:]' '[:lower:]')" \
  cargo test router::cms -- --nocapture --test-threads=1
cargo test controllers::dev_post::moderation_tests -- --nocapture --test-threads=1
cargo test --test dev_post_moderation_migration -- --nocapture
```

Expected: all focused suites pass. The router process has its own UUID prefix rather than relying on a service-module mutex; tests delete only exact prefixed keys and never connect to shared production Redis, flush, or scan.

- [ ] **Step 11: Dispatch the spec-compliance review**

Use `superpowers:requesting-code-review` with this exact scope:

```text
Check every approved design section, especially admin-first privacy, restore pin branching,
commit reconciliation, exact legacy+v2 cache invalidation, ranking generation bypass,
pgactive claim boundaries, migration draft-to-ready workflow, and OpenAPI 13+2 counts.
Report Critical/Important/Minor findings with file and line.
```

Expected: a complete spec-compliance report, not implementation changes.

- [ ] **Step 12: Dispatch the code-quality review**

Use `superpowers:requesting-code-review` with this exact scope:

```text
Check transaction consumption/rollback, SQL lock order, UUID/audit matching, Redis
captured-generation races, per-process Redis isolation plus supplemental serialization, Axum middleware order,
test isolation, exact cache keys, documentation negative assertions, and no unrelated diff.
Report Critical/Important/Minor findings with file and line.
```

Expected: a complete code-quality report, independent of Step 11.

- [ ] **Step 13: Resolve review findings and re-run the full gate**

For each Critical/Important finding, first add the named failing test beside the affected module, run its exact fully-qualified Cargo test, apply the smallest `apply_patch`, rerun that test, then repeat Steps 1–12. If the test touches Redis, invoke it with the exact `REDIS_TEST_URL` guard and a new UUID `REDIS_KEY_PREFIX` command form from Step 9; never run it against `.env` Redis without that override. Stage only files named by the finding and commit with `fix(cms): address moderation review findings`. Minor findings may be left only when the PR handoff lists the exact file/line and rationale.

- [ ] **Step 14: Push the reviewed API feature branch**

```bash
git status --short
git log --oneline v2..HEAD
git push -u origin feat/cms-dev-post-moderation
```

Expected: clean worktree before push and the remote feature branch contains only the reviewed commits.

- [ ] **Step 15: Open and verify the Draft API PR**

```bash
gh pr create \
  --base v2 \
  --head feat/cms-dev-post-moderation \
  --draft \
  --title "feat(cms): add audited Dev Post moderation" \
  --body "Adds idempotent CMS Dev Post delete/restore, permanent moderation audit, shared author/admin soft-delete core, commit reconciliation, complete cache invalidation, generation-scoped ranking cache, Swagger/docs, and concurrency coverage. The additive migrations PR was merged first and this branch points at its final origin/v2 squash commit."
gh pr view --json number,url,baseRefName,headRefName,isDraft
```

Expected: clean worktree, Draft PR base `v2`, correct head, migrations gitlink included. Report PR URL, migration commit, API commits, full test/check results, pgactive qualification status, and any accepted eventual-consistency limitation. Do not merge the API PR without a new user instruction.

## Final Verification Checklist

- [ ] The exact pushed Draft migration head passed the executable temporary-DB schema contract on explicit non-production `DATABASE_TEST_URL` and the fix/test/commit/push/re-review loop before user approval; only then was it marked ready, squash-merged without branch deletion, and synchronized with migrations `origin/v2`.
- [ ] API gitlink equals the final migrations squash commit; `migrations-test/0040_dev_post_moderation_log.sql` is the exact symlink.
- [ ] Audit table has UUID PK, post/action checks, post/admin indexes, no FK/default UUID/backfill.
- [ ] Authentication precedes path extraction; admin lock precedes all post access; EIP-55 comparison is exact.
- [ ] Author delete contract is unchanged, writes no CMS audit, and both author DELETE and CMS DELETE unpin atomically.
- [ ] CMS DELETE/RESTORE are idempotent for existing rows, audit every 204, and preserve 403 existence privacy.
- [ ] Orphan DELETE succeeds; orphan RESTORE is 409 without audit.
- [ ] Actual restore clears stale historical pin and returns the post as ordinary; live no-op restore preserves its current pin and all relationship rows/timestamps, including a concurrent unlike of a separate existing like.
- [ ] Pre-commit failure rolls back pin/post/audit; commit errors reconcile only by exact audit UUID/context.
- [ ] Unknown author/CMS commit outcomes attempt full invalidation and retain `outcome_unknown` classification.
- [ ] Full invalidation independently covers global/token legacy+v2 feed, detail, trending, ranking generation.
- [ ] A confirmed CMS completion remains successful when an injected ranking-generation bump fails, while all exact payload families are still cleared.
- [ ] Ranking reads generation once, missing means zero, errors bypass, captured N never writes N+1, and legacy payloads are ignored.
- [ ] No `LOWER()`, Redis enumeration/flush, request body, audit reason, hard delete, or moderation-log read API was added.
- [ ] Same-primary delete/restore assertions derive final state from returned `changed` contexts and exact audit UUID rows, never timestamp or random-UUID ordering; pgactive assertions remain convergence-compatible.
- [ ] Every Redis-touching Cargo process uses a dedicated non-production `REDIS_TEST_URL`, a fresh asserted UUID prefix, and exact prefixed keys; router processes do not rely on the service-test mutex.
- [ ] Runtime paths use `:post_id`, docs paths use `{post_id}`, 204 bodies are empty, Swagger/Scalar expose both routes.
- [ ] Dev Post endpoint count remains 13; two routes are counted as CMS moderation endpoints.
- [ ] Rollout and rollback require old-node drain, replica-lag check, exact prefixed trending DEL, and exact prefixed generation INCR before controls are enabled.
- [ ] Full fmt/check/test/clippy matrix and two-stage review are green before the Draft PR is handed off.

## Plan Self-Review Record

- **Spec coverage:** Sections 1–5 map to Tasks 7, 8, 10, and 15; Sections 6–9 map to Tasks 3, 6, 7, 11, and 12; Sections 10–11 map to Tasks 4, 5, 8, and 9; Sections 12–13 map to Tasks 8, 13, and 14; migration Section 14 maps to Tasks 2–3; OpenAPI/docs Section 15 maps to Tasks 10 and 15; test Section 16 maps across Tasks 3–15; rollout/rollback Sections 17–18 map to Task 15 and the final checklist.
- **No-placeholder audit:** the plan contains no deferred marker, implementation deferral, wildcard error-handling instruction, or unnamed file/function. Conditional migration renumbering is an explicit stop condition, not deferred design.
- **Dependency/command audit:** every referenced crate is already declared (`sqlx 0.8`, `redis 0.29`, `tokio`, `tower`, `utoipa`, `chrono`, and `uuid` with `v4`); no Cargo dependency step is missing. The exact command dependencies `cargo`, `rg`, `gh`, and `/usr/bin/uuidgen` are present, and database/Redis commands fail closed unless their explicit non-production test URLs are supplied.
- **Granularity/fence audit:** the plan has 16 reviewable tasks, 112 executable step checkboxes, and 20 final-verification checkboxes; each step is one exact patch, command gate, review dispatch, commit, or PR action. All 248 code-fence markers are paired and `git diff --check` is clean.
- **Type consistency:** controller outcomes use `CommitOutcome<T>` throughout; CMS reconciliation uses `ModerationContext`/`ModerationAudit`; the confirmed injected-bump test enters through `finish_cms_unknown_after_lookup_with_bump` with an exact matching audit; cache fixtures carry `SeededPublicCaches`; service methods are `delete_post_as_admin` and `restore_post_as_admin`; Redis methods consistently include `for_generation`; handlers call the same service names and return `AppResult<StatusCode>`.
- **Boundary check:** no moderation SQL is assigned to `CmsController`; no unrelated create/edit/like/vote/pin cache policy is broadened; no CMS list/search/audit-read UI is introduced.
