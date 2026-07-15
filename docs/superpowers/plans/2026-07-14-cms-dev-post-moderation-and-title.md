# CMS Dev Post Moderation and Explicit Title Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship audited CMS Dev Post delete/restore and an immediate breaking `title`/description-only `body` contract in one API PR, with permanent audit data, deterministic legacy backfill, safe Redis namespace cutover, and operationally bounded eventual consistency.

**Architecture:** Two ordered migrations add the moderation audit table and then split legacy content; because both are unpublished on migrations `origin/v2` at planning time, they are qualified and published together in one migrations branch/PR. `DevPostController` owns title-aware SQL plus primary-database locks, mutations, audit insertion, rollback, and commit outcome capture; `DevPostService` owns commit reconciliation, structured logs, captured-generation ranking, and best-effort exact-key invalidation. The HTTP cutover is intentionally immediate after a maintenance gate: no old-client negotiation, fallback payload, or versioned route is added.

**Tech Stack:** Rust 2024, Axum 0.7, Serde, sqlx 0.8/PostgreSQL, Redis 0.29, UUID v4, tracing, utoipa/OpenAPI, `#[sqlx::test]`, pgactive production writers.

**Approved designs:** `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md`, `docs/superpowers/specs/2026-07-14-dev-post-title-design.md`

## Global Constraints

- API integration base and PR base are `v2`; fetch the latest `origin/v2` before creating the implementation branch and after every merge.
- The current worktree branch is documentation-only. Per `AGENTS.md`, create a fresh `feat/dev-post-moderation-title` branch from updated `v2`; do not open the API PR from `feat/cms-dev-post-moderation`.
- `migrations/` is a separate repository/submodule. At planning time migrations `origin/v2` is `ef05bcc` and ends at `0039_dev_post_pin.sql`; therefore `0040_dev_post_moderation_log.sql` and `0041_dev_post_title.sql` are both unpublished and belong to one migrations feature branch/Draft PR.
- Recheck canonical migration numbering from migrations `origin/v2` immediately before creating SQL and again immediately before marking the Draft PR ready. A filename present in `origin/v2` is published and immutable: never rename, amend, reuse, or reorder it. If a collision lands, retain every published number and assign only the remaining unpublished work the next canonical number in dependency order, then update the specs and this plan in a documentation-only commit before resuming.
- Never merge the migrations PR without explicit user approval after reporting the PR URL, exact pushed head SHA, disposable-database result, and review result. After approval, run `gh pr ready`, verify `isDraft=false`, squash-merge without `--delete-branch`, then run `git fetch origin v2:v2` inside the migrations repository and verify local `v2 == origin/v2`.
- EVM addresses are canonical EIP-55 strings. Compare exact strings with SQL `=`; never introduce `LOWER()`, `lower()`, `ILIKE`, case folding, or a case-insensitive address index.
- `dev_post.title` is `TEXT NOT NULL DEFAULT ''`; API create requires a present non-null title whose `trim()` is nonempty. Validation does not modify the accepted string and adds no title-specific length limit or OpenAPI `maxLength`.
- Create missing/null/empty/whitespace title is `400`. Edit title is wire-tristate: omitted preserves, a nonblank string replaces exactly, and explicit null/empty/whitespace is `400`. A title-only edit is valid and marks the post edited.
- Legacy backfill splits once at the first LF; an immediately preceding CR is consumed with that LF. No other trimming/normalization occurs. The migration runner's applied version is the only split guard; no rerunnable operator backfill is created.
- The old client break is accepted. No capability header, client-version negotiation, `/v2/dev-post`, compatibility response, concatenated fallback `body`, or old-payload read fallback is introduced.
- CMS DELETE/RESTORE accepts no request body and returns a byte-empty `204` for existing-post success, including no-ops. Auth runs before extraction: unauthenticated malformed IDs/JSON are `401`; authenticated malformed/nonpositive IDs are `400`; non-admin valid positive IDs are `403` before post lookup; absent rows are `404`; orphan restore is `409`.
- Admin authorization, token/post locks, state change, pin cleanup, and audit insert use the write pool in one transaction. Same-primary lock order is CMS `admin -> token -> post`; author/pin remains `token -> post`.
- Moderation preserves `title`, `body`, author, images, likes, poll/options/votes, and original timestamps. A true restore clears only `deleted_at` and an impossible historical pin; a live no-op restore preserves a current pin.
- Pre-commit failures explicitly roll back best-effort and perform no invalidation. A `COMMIT` error is outcome-unknown: CMS reconciles an exact immutable audit row on the write pool; author delete cannot reconcile but still invalidates and returns the existing `500` status with `outcome_unknown` classification.
- Audit/log output contains identifiers, action, and `changed` only; it never contains title, body, image/poll content, cookie/session/auth values, request bodies, or IP addresses.
- New payload readers/writers use only `devpost:feed:v3:{scope}`, `devpost:detail:v2:{post_id}`, and `devpost:trending:v2`. Ranking uses `devpost:ranking:generation` plus `devpost:ranking:v2:{generation}:{page}:{limit}` and captures generation exactly once.
- Exact invalidation independently attempts every applicable legacy/new payload key. Create, author delete, CMS delete, and CMS restore bump ranking generation; title/body/image edit and pin/unpin do not. A Redis failure never reverses a confirmed PostgreSQL result.
- Never use Redis `KEYS`, `SCAN`, wildcard deletion, prefix-wide deletion, or `FLUSHALL` in feature source, tests, or rollout commands. Every Redis command requires `REDIS_TEST_CONFIRM_DISPOSABLE=1`, a nonempty dedicated `REDIS_TEST_URL`, inequality with any ambient production `REDIS_URL` before override, and a fresh nonempty UUID `REDIS_KEY_PREFIX`.
- Feed/detail/trending/ranking TTL configuration must be at most `60_000ms`. Deterministic late-fill tests use barriers/injected ordering, assert positive bounded `PTTL`, and never sleep for a production TTL.
- Same-primary tests may assert row-lock serialization. pgactive tests assert unique audit attempts, valid local observations, eventual convergence, and safe retry only; they never claim one global winner, one global `changed=true`, global timestamp order, or last-request-wins.
- pgactive qualification may run only when both writer URLs are nonempty and the operator sets `PGACTIVE_TEST_CONFIRM_DISPOSABLE=1` for migrated disposable writers; absence is a failed qualification gate, not evidence of pgactive correctness.
- Every PostgreSQL test command requires `DATABASE_TEST_CONFIRM_DISPOSABLE=1`, a
  nonempty disposable non-production `DATABASE_TEST_URL`, and inequality with any
  ambient production `DATABASE_URL` before override. Only then may the command override
  `DATABASE_URL="$DATABASE_TEST_URL"`; never qualify Draft SQL against production.
- Use `apply_patch` for source edits, preserve unrelated work, stage only named files, and end every implementation task with focused tests, `cargo fmt --all -- --check`, `git diff --check`, and a scoped commit.

---

## File Responsibility Map

- `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md` — approved moderation decisions, linked to this integrated execution plan and corrected for title/cache/migration cutover.
- `docs/superpowers/specs/2026-07-14-dev-post-title-design.md` — approved title decisions, linked to this integrated execution plan and corrected for the one-PR unpublished migration case.
- `docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md` — historical moderation-only plan, permanently marked superseded and non-executable.
- `migrations/0040_dev_post_moderation_log.sql` — permanent UUID audit table, checks, and two lookup indexes; no foreign keys or backfill.
- `migrations/0041_dev_post_title.sql` — `title` column plus one-time first-line/CRLF-aware legacy split.
- `migrations-test/0040_dev_post_moderation_log.sql`, `migrations-test/0041_dev_post_title.sql` — symlinks to the two final merged production migrations.
- `tests/dev_post_content_migrations.rs` — executable audit schema, title schema/backfill, relationship preservation, and migration-chain identity contract.
- `src/types/dev_post/mod.rs` — create required title, edit presence-aware title, validation, required response title, and serde/schema fixtures.
- `src/result.rs` — JSON-envelope `PayloadTooLarge` error mapping that preserves authenticated `413`.
- `src/controllers/dev_post/mod.rs` — title-aware create/edit/hydration and registration of the focused moderation module.
- `src/controllers/dev_post/moderation.rs` — shared author/admin soft-delete core, admin-first locks, restore, audit insertion, rollback, commit outcome, and write-pool reconciliation lookup.
- `src/controllers/dev_post/moderation_tests.rs` — idempotency, privacy, rollback, preservation, lock/concurrency, and opt-in pgactive tests with explicit title fixtures.
- `src/controllers/dev_post/pin.rs`, `src/controllers/dev_post/pin_feed_tests.rs` — existing token-before-post behavior and title-aware fixtures; pin authorization stays unchanged.
- `src/config.rs` — four-cache TTL ceiling validation.
- `src/db/redis/mod.rs` — v3/v2 payload keys, exact multi-version deletion, captured ranking-generation primitives, and Redis contract tests.
- `src/services/dev_post/mod.rs` — title-aware write invalidation, versioned cache-aside reads, ranking capture, author/CMS orchestration, reconciliation, and content-free operational logs.
- `src/services/dev_post/moderation_tests.rs` — commit reconciliation, exact invalidation, no-op repair, Redis-failure isolation, late-fill barriers, and generation race tests.
- `src/router/dev_post/handler.rs` — JSON rejection mapping that distinguishes `400` from `413` and preserves auth-before-extraction.
- `src/router/dev_post/mod.rs` — real-router title/auth/body-limit precedence tests.
- `src/router/cms/path.rs` — distinct runtime `:post_id` and docs `{post_id}` paths.
- `src/router/cms/handler.rs` — thin CMS delete/restore handlers returning `AppResult<StatusCode>`.
- `src/router/cms/mod.rs` — authenticated route registration and real-router status/empty-body tests.
- `src/main.rs` — OpenAPI path/schema registration, 13 Dev Post + 2 CMS operation assertions, and no-negotiation contract checks.
- `docs/dev-post-api.md`, `docs/V2_API_CHANGES.md`, `docs/cms-dev-post-moderation.md` — title/body API contract, CMS operations/audit behavior, maintenance rollout, point of no return, rollback, and monitoring.

### Task 1: Supersede stale documents and approve one integrated execution source

**Files:**
- Modify: `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md`
- Modify: `docs/superpowers/specs/2026-07-14-dev-post-title-design.md`
- Modify: `docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md`
- Verify: `docs/superpowers/plans/2026-07-14-cms-dev-post-moderation-and-title.md`

**Interfaces:**
- Consumes: both approved designs and the user's decision to merge both features in one API PR and, while both migrations remain unpublished, one migrations PR.
- Produces: exactly one executable plan link; the two specs remain decision records and the old plan cannot be mistaken for runnable work.

- [ ] **Step 1: Prove only documentation is dirty before changing execution authority**

```bash
git status --short --branch
git diff --name-only
```

Expected: the worktree is on `feat/cms-dev-post-moderation`; no production Rust, test, migration, or submodule change is present.

- [ ] **Step 2: Replace the moderation spec banner and status**

Use `apply_patch` to replace its current caution block and status with this exact text:

```markdown
> [!IMPORTANT]
> **APPROVED DECISION RECORD — EXECUTE ONLY THE INTEGRATED PLAN.** The moderation
> decisions in this document remain authoritative, but implementation is unified with
> explicit title/body separation by
> [`../plans/2026-07-14-cms-dev-post-moderation-and-title.md`](../plans/2026-07-14-cms-dev-post-moderation-and-title.md).
> Do not execute the historical moderation-only plan.

- Status: Approved decision record; superseded for execution by the integrated plan
```

Also revise Sections 1–3, 8, 10–11, 14–18 so they say moderation preserves both `title` and `body`, uses feed v3/detail v2/trending v2 plus ranking generation, shares the accepted late-fill/replica race, uses one migrations PR only while both files are unpublished, exposes 13 Dev Post + 2 CMS operations, and follows the title migration point-of-no-return rollback boundary.

- [ ] **Step 3: Add the title-spec execution banner and reconcile its migration workflow**

Insert this exact block below the title:

```markdown
> [!IMPORTANT]
> **APPROVED DECISION RECORD — EXECUTE ONLY THE INTEGRATED PLAN.** The title/body
> decisions in this document remain authoritative. The approved combined implementation,
> including the one-PR path while both migrations are unpublished, is
> [`../plans/2026-07-14-cms-dev-post-moderation-and-title.md`](../plans/2026-07-14-cms-dev-post-moderation-and-title.md).
```

Change Section 5.3 so `0040_dev_post_moderation_log.sql` and `0041_dev_post_title.sql` are created, qualified, reviewed, explicitly approved, and squash-merged together when neither exists on execution-time migrations `origin/v2`. Preserve the fallback rule verbatim: any filename already in `origin/v2` is published and immutable; only unpublished remaining work moves to the next canonical number.

- [ ] **Step 4: Permanently supersede the old plan**

Replace its current caution block with:

```markdown
> [!CAUTION]
> **SUPERSEDED — DO NOT EXECUTE.** This moderation-only plan predates explicit
> `title`/description-only `body`. Use
> [`2026-07-14-cms-dev-post-moderation-and-title.md`](2026-07-14-cms-dev-post-moderation-and-title.md)
> for all migration, implementation, test, rollout, rollback, review, and PR work.
```

Do not rewrite its historical 16 tasks; the banner and integrated link are the durable guard.

- [ ] **Step 5: Run document consistency and source-scope checks**

```bash
rg -n 'EXECUTE ONLY THE INTEGRATED PLAN|SUPERSEDED — DO NOT EXECUTE|cms-dev-post-moderation-and-title' \
  docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md \
  docs/superpowers/specs/2026-07-14-dev-post-title-design.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md
git diff --name-only | rg -v '^docs/superpowers/(specs|plans)/' && exit 1 || true
git diff --check
```

Expected: all three stale documents link to this plan; only documentation paths changed; whitespace check is silent.

- [ ] **Step 6: Run two document reviews before any migration or executable test is created**

Use `superpowers:requesting-code-review` twice. Assign the first review to **Opus 4.8** for design/spec compliance:

```text
Compare both approved decision records with the integrated plan. Verify title create/edit
wire states, first-LF/CRLF backfill, immediate client break, CMS privacy/idempotency/audit,
commit reconciliation, v3/v2 cache namespaces, ranking generation, migration publication
fallback, rollout point of no return, and rollback. Report Critical/Important/Minor with lines.
```

Assign the independent planning second opinion to **Fable 5**:

```text
Verify exact files/interfaces, 2–5 minute checkbox actions, TDD RED/GREEN order, disposable
Postgres/Redis guards, immutable published migrations, explicit migration approval pause,
source-only safety scans, review gates, and Draft API PR handoff. Report findings with lines.
```

Resolve every Critical/Important finding with `apply_patch`, rerun Step 5, and repeat both reviews until neither reports a Critical/Important issue.

- [ ] **Step 7: Commit the execution-authority reconciliation**

```bash
git add \
  docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md \
  docs/superpowers/specs/2026-07-14-dev-post-title-design.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation.md \
  docs/superpowers/plans/2026-07-14-cms-dev-post-moderation-and-title.md
git diff --cached --check
git commit -m "docs: approve integrated Dev Post moderation and title plan"
```

Expected: one documentation-only commit; no implementation begins before it and both reviews are green.

### Task 2: Qualify and publish the ordered audit/title migrations in one gated PR

**Files:**
- Create in migrations repository: `0040_dev_post_moderation_log.sql`
- Create in migrations repository: `0041_dev_post_title.sql`
- Create: `migrations-test/0040_dev_post_moderation_log.sql`
- Create: `migrations-test/0041_dev_post_title.sql`
- Create: `tests/dev_post_content_migrations.rs`
- Modify: `migrations` gitlink

**Interfaces:**
- Consumes: latest API/migrations `v2`; planning-time canonical last migration `0039_dev_post_pin.sql`.
- Produces: final migrations squash commit containing audit schema followed by title schema/backfill; API gitlink and test symlinks point at that exact published commit.

- [ ] **Step 1: Create the fresh API implementation branch and prove the baseline**

```bash
DOC_TIP="$(git rev-parse HEAD)"
git fetch origin v2:v2
test "$(git ls-remote origin refs/heads/v2 | cut -f1)" = "$(git rev-parse v2)"
test -z "$(git status --porcelain)"
git show-ref --verify --quiet refs/heads/feat/dev-post-moderation-title && {
  echo 'feat/dev-post-moderation-title already exists; inspect instead of overwriting' >&2
  exit 1
} || true
git switch -c feat/dev-post-moderation-title v2
git cherry-pick $(git rev-list --reverse "v2..${DOC_TIP}")
git merge-base --is-ancestor v2 HEAD
git submodule update --init migrations
cargo fmt --all -- --check
cargo check --all-targets --all-features
```

Expected: a new branch based on current `v2`, containing only the reviewed documentation commits; baseline formatting and compilation pass.

- [ ] **Step 2: Recheck canonical migration state and apply the immutable-publication fallback**

```bash
git -C migrations fetch origin v2:v2
REMOTE_TREE="$(mktemp)"
git -C migrations ls-tree --name-only origin/v2 \
  | rg '^[0-9]{4}_[a-z0-9][a-z0-9_]*\.sql$' \
  | rg -v '^1000_delete\.sql$' \
  | LC_ALL=C sort > "$REMOTE_TREE"
tail -5 "$REMOTE_TREE"
test "$(tail -1 "$REMOTE_TREE")" = '0039_dev_post_pin.sql'
test -z "$(rg '^0040_|^0041_' "$REMOTE_TREE" || true)"
rm "$REMOTE_TREE"
```

Expected at the approved planning state: `0039_dev_post_pin.sql` is last and neither `0040_` nor `0041_` is published, so both files stay in one PR.

If either number now exists, stop before creating or renaming SQL. Treat every path printed from `origin/v2` as immutable. If a published file already implements one approved migration exactly, verify its content/ancestry and create only the missing migration at the next canonical number. If it is unrelated, assign moderation and then title to the next two free numbers. Update both specs, this plan, symlink names, test assertions, and PR text in a documentation-only commit; never modify the published file or its commit.

- [ ] **Step 3: Create the migrations feature branch and write the first failing contract**

```bash
git -C migrations switch -c feat/dev-post-moderation-title origin/v2
```

Use `apply_patch` to create `tests/dev_post_content_migrations.rs` first:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn audit_and_title_migrations_are_required(pool: sqlx::PgPool) {
    let audit_exists: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.dev_post_moderation_log') IS NOT NULL",
    ).fetch_one(&pool).await.unwrap();
    let title_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='dev_post' AND column_name='title')",
    ).fetch_one(&pool).await.unwrap();
    assert!(audit_exists);
    assert!(title_exists);
}
```

- [ ] **Step 4: Run migration contract RED before writing SQL**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  --test dev_post_content_migrations audit_and_title_migrations_are_required -- --nocapture
```

Expected RED: `audit_exists` and `title_exists` are false on the canonical chain ending at `0039`; no `0040`/`0041` production SQL exists yet.

#### Step 5 reference: Complete pre-implementation migration contract

Before creating either SQL file, use `apply_patch` to replace the minimal RED test in
`tests/dev_post_content_migrations.rs` with these complete assertions:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn audit_and_title_migrations_have_exact_contract(pool: sqlx::PgPool) {
    let title: (String, String, String) = sqlx::query_as(
        "SELECT data_type, is_nullable, column_default FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='dev_post' AND column_name='title'",
    )
    .fetch_one(&pool).await.unwrap();
    assert_eq!(title, ("text".into(), "NO".into(), "''::text".into()));

    let title_indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_index i \
         JOIN pg_class t ON t.oid=i.indrelid \
         JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=ANY(i.indkey) \
         WHERE t.relname='dev_post' AND a.attname='title'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(title_indexes, 0);

    let audit_id_default: Option<String> = sqlx::query_scalar(
        "SELECT column_default FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='dev_post_moderation_log' \
           AND column_name='id'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(audit_id_default, None, "UUID must be application-generated");

    let audit_fks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.table_constraints \
         WHERE table_schema='public' AND table_name='dev_post_moderation_log' \
           AND constraint_type='FOREIGN KEY'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(audit_fks, 0);

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname='public' \
         AND tablename='dev_post_moderation_log' ORDER BY indexname",
    ).fetch_all(&pool).await.unwrap();
    assert!(indexes.contains(&"idx_dev_post_moderation_log_post_created".into()));
    assert!(indexes.contains(&"idx_dev_post_moderation_log_admin_created".into()));

    for (index, expected) in [
        ("idx_dev_post_moderation_log_post_created",
            vec!["post_id".to_string(), "created_at".to_string()]),
        ("idx_dev_post_moderation_log_admin_created",
            vec!["admin_account_id".to_string(), "created_at".to_string()]),
    ] {
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT a.attname FROM pg_index i \
             JOIN pg_class idx ON idx.oid=i.indexrelid \
             CROSS JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS key(attnum, ord) \
             JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=key.attnum \
             WHERE idx.relname=$1 ORDER BY key.ord",
        ).bind(index).fetch_all(&pool).await.unwrap();
        assert_eq!(columns, expected);
        let definition: String = sqlx::query_scalar(
            "SELECT pg_get_indexdef(indexrelid) FROM pg_index i \
             JOIN pg_class idx ON idx.oid=i.indexrelid WHERE idx.relname=$1",
        ).bind(index).fetch_one(&pool).await.unwrap();
        let leading_column = if index == "idx_dev_post_moderation_log_post_created" {
            "post_id"
        } else {
            "admin_account_id"
        };
        assert!(definition.contains(
            &format!("({leading_column}, created_at DESC)")
        ), "unexpected index direction: {definition}");
    }

    let no_backfill: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(no_backfill, 0);
    let app_uuid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,1,'0x0000000000000000000000000000000000000001', \
                 '0x0000000000000000000000000000000000000002','DELETE',true)",
    ).bind(app_uuid).execute(&pool).await.unwrap();
    let stored_uuid: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM dev_post_moderation_log WHERE id=$1",
    ).bind(app_uuid).fetch_one(&pool).await.unwrap();
    assert_eq!(stored_uuid, app_uuid);
    assert!(sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,0,'0x1','0x2','DELETE',false)",
    ).bind(uuid::Uuid::new_v4()).execute(&pool).await.is_err());
    assert!(sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,2,'0x1','0x2','EDIT',false)",
    ).bind(uuid::Uuid::new_v4()).execute(&pool).await.is_err());

    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO public.dev_post(token_id,author,title,body) \
         VALUES ('0x0000000000000000000000000000000000000003', \
                 '0x0000000000000000000000000000000000000004','Audit survivor','body') \
         RETURNING id",
    ).fetch_one(&pool).await.unwrap();
    let survivor_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,$2,'0x0000000000000000000000000000000000000003', \
                 '0x0000000000000000000000000000000000000004','DELETE',true)",
    ).bind(survivor_id).bind(post_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM public.dev_post WHERE id=$1")
        .bind(post_id).execute(&pool).await.unwrap();
    let survivor_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM dev_post_moderation_log WHERE id=$1",
    ).bind(survivor_id).fetch_one(&pool).await.unwrap();
    assert_eq!(survivor_count, 1);
}

#[sqlx::test(migrations = "./migrations-test")]
async fn legacy_split_is_first_lf_crlf_aware_and_relationship_safe(pool: sqlx::PgPool) {
    sqlx::raw_sql(
        "CREATE TEMP TABLE dev_post (id BIGSERIAL PRIMARY KEY, body TEXT NOT NULL); \
         CREATE TEMP TABLE dev_post_image (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll_option (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_like (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll_vote (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_pin (post_id BIGINT);",
    ).execute(&pool).await.unwrap();
    let cases = [
        ("Title\nDescription", "Title", "Description"),
        ("Title\nLine 1\nLine 2", "Title", "Line 1\nLine 2"),
        ("Title", "Title", ""),
        ("", "", ""),
        ("\nDescription", "", "Description"),
        ("Title\r\nDescription", "Title", "Description"),
        ("Title\rDescription", "Title\rDescription", ""),
    ];
    for (position, (legacy, _, _)) in cases.iter().enumerate() {
        let id: i64 = sqlx::query_scalar("INSERT INTO dev_post(body) VALUES ($1) RETURNING id")
            .bind(legacy).fetch_one(&pool).await.unwrap();
        assert_eq!(id, position as i64 + 1);
    }
    sqlx::raw_sql(
        "INSERT INTO dev_post_image VALUES (1); INSERT INTO dev_post_poll VALUES (1); \
         INSERT INTO dev_post_poll_option VALUES (1); INSERT INTO dev_post_like VALUES (1); \
         INSERT INTO dev_post_poll_vote VALUES (1); INSERT INTO dev_post_pin VALUES (1);",
    ).execute(&pool).await.unwrap();
    let ids_before: Vec<i64> = sqlx::query_scalar("SELECT id FROM dev_post ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    let count_before = ids_before.len();
    sqlx::raw_sql(include_str!("../migrations/0041_dev_post_title.sql"))
        .execute(&pool).await.unwrap();
    let ids_after: Vec<i64> = sqlx::query_scalar("SELECT id FROM dev_post ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(ids_after, ids_before);
    assert_eq!(ids_after.len(), count_before);
    let actual: Vec<(String, String)> =
        sqlx::query_as("SELECT title, body FROM dev_post ORDER BY id")
            .fetch_all(&pool).await.unwrap();
    let expected: Vec<(String, String)> = cases.iter()
        .map(|(_, title, body)| ((*title).into(), (*body).into())).collect();
    assert_eq!(actual, expected);
    let relation_counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM dev_post_image), \
                (SELECT count(*) FROM dev_post_poll), \
                (SELECT count(*) FROM dev_post_poll_option), \
                (SELECT count(*) FROM dev_post_like), \
                (SELECT count(*) FROM dev_post_poll_vote), \
                (SELECT count(*) FROM dev_post_pin)",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(relation_counts, (1, 1, 1, 1, 1, 1));
}
```

The temporary `dev_post` shadows the migrated public table on that connection, so the exact production `0041` file runs once against pre-title fixture rows without attempting a second split of public data. The six temporary relationship tables prove that file does not mutate them.

Create symlinks with exact relative targets and record the temporary branch gitlink:

```bash
ln -s ../migrations/0040_dev_post_moderation_log.sql migrations-test/0040_dev_post_moderation_log.sql
ln -s ../migrations/0041_dev_post_title.sql migrations-test/0041_dev_post_title.sql
test "$(readlink migrations-test/0040_dev_post_moderation_log.sql)" = '../migrations/0040_dev_post_moderation_log.sql'
test "$(readlink migrations-test/0041_dev_post_title.sql)" = '../migrations/0041_dev_post_title.sql'
```

- [ ] **Step 5a: Add title-column and audit metadata assertions**

Patch only the title type/default/index and audit UUID/FK/check/index assertions from the
reference contract, then run `git diff --check -- tests/dev_post_content_migrations.rs`.

- [ ] **Step 5b: Add audit survival assertions**

Patch the application-generated UUID round-trip and post-deletion audit-survival portion,
then run `git diff --check -- tests/dev_post_content_migrations.rs`.

- [ ] **Step 5c: Add first-LF/CRLF and relationship-preservation assertions**

Patch the table-driven legacy split test and its exact relation-count assertion, then run
`git diff --check -- tests/dev_post_content_migrations.rs`.

- [ ] **Step 5d: Add the pre-implementation exact symlinks**

Run only the reference `ln -s` and `readlink` commands; both `readlink` checks must pass
before the detailed RED. The symlinks intentionally point to absent SQL files at this
stage, so the test proves the production migrations are still missing.

- [ ] **Step 6: Run the detailed migration contract RED before writing SQL**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  --test dev_post_content_migrations -- --nocapture
```

Expected RED: compilation/migration loading fails because both exact symlink targets are
absent. The already-written contract includes title type/default/index, audit UUID/FK/
checks/indexes/survival, first-LF/CRLF/bare-CR cases, row identity, and relationship
preservation before implementation exists.

- [ ] **Step 7: Write the minimal audit migration**

Use `apply_patch` in `migrations/0040_dev_post_moderation_log.sql`:

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

- [ ] **Step 8: Write the ordered title schema and one-time split**

Use `apply_patch` in `migrations/0041_dev_post_title.sql`:

```sql
ALTER TABLE dev_post
    ADD COLUMN title TEXT NOT NULL DEFAULT '';

WITH legacy AS (
    SELECT id, body AS legacy_body, strpos(body, E'\n') AS first_lf
    FROM dev_post
)
UPDATE dev_post AS dp
SET title = CASE
        WHEN legacy.first_lf = 0 THEN legacy.legacy_body
        ELSE left(
            legacy.legacy_body,
            legacy.first_lf - 1
                - CASE
                    WHEN legacy.first_lf > 1
                     AND substr(legacy.legacy_body, legacy.first_lf - 1, 1) = E'\r'
                    THEN 1 ELSE 0
                  END
        )
    END,
    body = CASE
        WHEN legacy.first_lf = 0 THEN ''
        ELSE substr(legacy.legacy_body, legacy.first_lf + 1)
    END
FROM legacy
WHERE dp.id = legacy.id;
```

Expected: schema and split share the numbered migration transaction; bare CR remains content and only the first LF delimits.

- [ ] **Step 9: Commit and push the exact migrations Draft head**

```bash
git -C migrations add 0040_dev_post_moderation_log.sql 0041_dev_post_title.sql
git -C migrations diff --cached --check
git -C migrations commit -m "feat(dev-post): add moderation audit and explicit titles"
git -C migrations push -u origin feat/dev-post-moderation-title
DRAFT_HEAD="$(git -C migrations rev-parse HEAD)"
REMOTE_HEAD="$(git -C migrations ls-remote origin refs/heads/feat/dev-post-moderation-title | cut -f1)"
test "$DRAFT_HEAD" = "$REMOTE_HEAD"
```

Expected: the remote feature head exactly equals the locally reviewed two-file commit.

- [ ] **Step 10: Run GREEN against a freshly verified Draft head**

```bash
set -euo pipefail
LOCAL_DRAFT_HEAD="$(git -C migrations rev-parse HEAD)"
REMOTE_DRAFT_HEAD="$(git -C migrations ls-remote origin \
  refs/heads/feat/dev-post-moderation-title | cut -f1)"
test "$LOCAL_DRAFT_HEAD" = "$REMOTE_DRAFT_HEAD"
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test --test dev_post_content_migrations -- --nocapture
git diff --cached --check
```

Expected GREEN: every audit/schema/backfill/relationship assertion passes on disposable databases; no production connection is used.

- [ ] **Step 11: Review the two SQL files and open one Draft migrations PR**

Use `superpowers:requesting-code-review` with:

```text
Review exactly 0040_dev_post_moderation_log.sql and 0041_dev_post_title.sql at the pushed
head. Check no FKs/backfill in 0040; UUID/check/index contract; 0041 TEXT NOT NULL DEFAULT '';
first-LF and CRLF arithmetic; empty/leading-LF/bare-CR cases; one-time execution; row and
relationship preservation; and canonical numbering from origin/v2. Report findings with lines.
```

Fix any Critical/Important finding on the migrations branch, push, rerun Step 10 using
its freshly recomputed local/remote heads, and repeat review. Then:

```bash
gh pr create \
  --repo Naddotfun/migrations \
  --base v2 \
  --head feat/dev-post-moderation-title \
  --draft \
  --title "feat(dev-post): add moderation audit and explicit titles" \
  --body "Adds ordered 0040 moderation audit and 0041 title/body split migrations. Both were unpublished on origin/v2 and were qualified together against disposable PostgreSQL. The title split is first-LF/CRLF-aware and runs once through migration history."
MIGRATION_PR_NUMBER="$(gh pr list --repo Naddotfun/migrations --state open \
  --head feat/dev-post-moderation-title --json number --jq '.[0].number')"
MIGRATION_APPROVED_SHA="$(git -C migrations rev-parse HEAD)"
git -C migrations update-ref refs/codex/dev-post-migrations-reviewed "$MIGRATION_APPROVED_SHA"
gh pr view "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations \
  --json number,url,baseRefName,headRefName,isDraft,state,headRefOid
```

Expected: one open Draft PR based on migrations `v2`, with `headRefOid == MIGRATION_APPROVED_SHA`; local persistent ref `refs/codex/dev-post-migrations-reviewed` resolves to the same reviewed SHA.

- [ ] **Step 12: Pause for explicit user approval**

Report the migrations PR number and URL plus the SHA printed by `git -C migrations rev-parse refs/codex/dev-post-migrations-reviewed`, canonical-number recheck, disposable DB command/result, SQL review result, and the fact that the PR contains both unpublished ordered migrations. Ask the user to approve that exact PR number and printed SHA. Do not run `gh pr ready` or `gh pr merge` until the user explicitly approves both identities.

- [ ] **Step 13: After approval, recheck numbering, publish, synchronize, and pin the final gitlink**

```bash
set -euo pipefail
MIGRATION_PR_NUMBER="${MIGRATION_PR_NUMBER:?set the exact user-approved PR number}"
USER_APPROVED_SHA="${USER_APPROVED_SHA:?set the exact SHA quoted in user approval}"
REVIEWED_SHA="$(git -C migrations rev-parse refs/codex/dev-post-migrations-reviewed)"
test "$USER_APPROVED_SHA" = "$REVIEWED_SHA"
PR_JSON="$(gh pr view "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations \
  --json number,baseRefName,headRefName,isDraft,state,headRefOid)"
test "$(jq -r .number <<<"$PR_JSON")" = "$MIGRATION_PR_NUMBER"
test "$(jq -r .baseRefName <<<"$PR_JSON")" = v2
test "$(jq -r .state <<<"$PR_JSON")" = OPEN
test "$(jq -r .headRefOid <<<"$PR_JSON")" = "$REVIEWED_SHA"
HEAD_REF="$(jq -r .headRefName <<<"$PR_JSON")"
REMOTE_SHA="$(git -C migrations ls-remote origin "refs/heads/$HEAD_REF" | cut -f1)"
test "$REMOTE_SHA" = "$REVIEWED_SHA"
git -C migrations fetch origin v2:v2
PRE_MERGE_V2_SHA="$(git -C migrations rev-parse origin/v2)"
test -z "$(git -C migrations ls-tree --name-only origin/v2 | rg '^00(40|41)_' || true)"
gh pr ready "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations
test "$(gh pr view "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations \
  --json isDraft --jq .isDraft)" = false
gh pr merge "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations --squash
git -C migrations fetch origin v2:v2
MIGRATIONS_MERGED_SHA="$(git -C migrations rev-parse origin/v2)"
test "$MIGRATIONS_MERGED_SHA" != "$PRE_MERGE_V2_SHA"
test "$(gh pr view "$MIGRATION_PR_NUMBER" --repo Naddotfun/migrations \
  --json state --jq .state)" = MERGED
git -C migrations switch --detach "$MIGRATIONS_MERGED_SHA"
test "$(git rev-parse HEAD:migrations 2>/dev/null || true)" != "$MIGRATIONS_MERGED_SHA"
git add migrations migrations-test/0040_dev_post_moderation_log.sql \
  migrations-test/0041_dev_post_title.sql tests/dev_post_content_migrations.rs
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test --test dev_post_content_migrations -- --nocapture
git diff --cached --check
git commit -m "test(dev-post): qualify moderation and title migrations"
```

Expected: the one shell proves the explicitly approved PR/SHA still match the persistent review ref and remote branch before ready/merge; final submodule HEAD is a fresh merged `origin/v2` SHA; the API commit records that squash gitlink, not the reviewed feature SHA.

- [ ] **Step 14: Verify published ancestry and immutable filenames from fresh remote state**

```bash
set -euo pipefail
git -C migrations fetch origin v2:v2
FRESH_MERGED_SHA="$(git -C migrations rev-parse origin/v2)"
test "$(git rev-parse HEAD:migrations)" = "$FRESH_MERGED_SHA"
git -C migrations merge-base --is-ancestor ef05bcc "$FRESH_MERGED_SHA"
git -C migrations ls-tree --name-only "$FRESH_MERGED_SHA" \
  | rg '^0040_dev_post_moderation_log\.sql$|^0041_dev_post_title\.sql$'
git -C migrations diff --exit-code "$FRESH_MERGED_SHA" -- \
  0040_dev_post_moderation_log.sql 0041_dev_post_title.sql
```

Expected: both exact published files exist in the final squash commit ancestry and are unmodified after publication.

### Task 3: Define the title wire contract and exact JSON error precedence

**Files:**
- Modify: `src/types/dev_post/mod.rs`
- Modify: `src/result.rs`
- Modify: `src/router/dev_post/handler.rs`
- Test: unit tests in the same three modules

**Interfaces:**
- Produces `EditTitle::{Omitted, Null, Value(String)}`, `EditTitle::value(&self) -> Option<&str>`, and title-aware `CreateDevPostRequest`, `EditDevPostRequest`, and `DevPostResponse`.
- Produces `AppError::PayloadTooLarge(String)` and `map_devpost_json_rejection(JsonRejection) -> AppError` so JSON syntax/type/validation is `400` while body-limit rejection remains `413`.
- Later tasks consume `req.title.value()` for SQL and required response/cache serialization.

#### Step 1 reference: Serde, validation, and rejection-mapping tests

Add these tests to `src/types/dev_post/mod.rs`:

```rust
#[test]
fn create_title_is_required_non_null_non_blank_and_exact() {
    for json in [
        r#"{"token_id":"0xToken"}"#,
        r#"{"token_id":"0xToken","title":null}"#,
    ] {
        assert!(serde_json::from_str::<CreateDevPostRequest>(json).is_err());
    }
    for title in ["", " ", "\t\r\n"] {
        let request: CreateDevPostRequest = serde_json::from_value(serde_json::json!({
            "token_id": "0xToken", "title": title
        })).unwrap();
        assert_eq!(request.validate().unwrap_err(), "Title must not be blank");
    }
    let exact = "  Headline\nsecond title line  ";
    let request: CreateDevPostRequest = serde_json::from_value(serde_json::json!({
        "token_id": "0xToken", "title": exact
    })).unwrap();
    request.validate().unwrap();
    assert_eq!(request.title, exact);
    assert!(request.body.is_none());
}

#[test]
fn edit_title_distinguishes_omitted_null_and_value() {
    let omitted: EditDevPostRequest = serde_json::from_str(r#"{"body":"description"}"#).unwrap();
    let null: EditDevPostRequest = serde_json::from_str(r#"{"title":null}"#).unwrap();
    let value: EditDevPostRequest = serde_json::from_str(r#"{"title":"  exact\nvalue  "}"#).unwrap();
    assert_eq!(omitted.title, EditTitle::Omitted);
    assert_eq!(null.title, EditTitle::Null);
    assert_eq!(value.title, EditTitle::Value("  exact\nvalue  ".into()));
    assert_eq!(null.validate().unwrap_err(), "Title must not be null or blank");
    assert_eq!(
        serde_json::from_str::<EditDevPostRequest>(r#"{"title":"  "}"#)
            .unwrap().validate().unwrap_err(),
        "Title must not be null or blank"
    );
    value.validate().unwrap();
}

#[test]
fn title_only_create_and_edit_supersede_old_content_rules() {
    let create: CreateDevPostRequest = serde_json::from_str(
        r#"{"token_id":"0xToken","title":"Title"}"#,
    ).unwrap();
    create.validate().unwrap();
    let edit: EditDevPostRequest = serde_json::from_str(r#"{"title":"Replacement"}"#).unwrap();
    edit.validate().unwrap();
    let empty: EditDevPostRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(empty.validate().unwrap_err(), "Nothing to update");
}
```

In `src/result.rs`, add `payload_too_large_maps_to_413`, asserting the repository error
variant produces `StatusCode::PAYLOAD_TOO_LARGE`. In
`src/router/dev_post/handler.rs`, add `json_rejection_preserves_400_versus_413`, asserting
ordinary JSON syntax/type rejection maps to `BadRequest` while an over-limit rejection
maps to `PayloadTooLarge`. Write both named tests before adding the variant or mapping
helper.

- [ ] **Step 1a: Add the failing create-title test**

Add and run only `create_title_is_required_non_null_non_blank_and_exact`; confirm RED.

- [ ] **Step 1b: Add the failing edit-title tristate test**

Add and run only `edit_title_distinguishes_omitted_null_and_value`; confirm RED.

- [ ] **Step 1c: Add the failing title-only contract test**

Add and run only `title_only_create_and_edit_supersede_old_content_rules`; confirm RED.

- [ ] **Step 1d: Add the failing repository 413 mapping test**

Add and run only `payload_too_large_maps_to_413`; confirm the missing variant causes RED.

- [ ] **Step 1e: Add the failing JSON rejection precedence test**

Add and run only `json_rejection_preserves_400_versus_413`; confirm the missing mapper
causes RED.

- [ ] **Step 2: Run RED**

```bash
cargo test types::dev_post::tests::create_title_is_required -- --nocapture
cargo test types::dev_post::tests::edit_title_distinguishes -- --nocapture
cargo test result::error_status_tests::payload_too_large_maps_to_413 -- --nocapture
cargo test router::dev_post::handler::tests::json_rejection_preserves_400_versus_413 -- --nocapture
```

Expected: compile failures because title fields, `EditTitle`, `PayloadTooLarge`, and the
JSON rejection mapper do not exist.

- [ ] **Step 3: Implement presence-aware title types and validation**

Use this exact public/internal contract in `src/types/dev_post/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EditTitle {
    #[default]
    Omitted,
    Null,
    Value(String),
}

impl EditTitle {
    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Value(value) => Some(value),
            Self::Omitted | Self::Null => None,
        }
    }
}

fn deserialize_edit_title<'de, D>(deserializer: D) -> Result<EditTitle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Option::<String>::deserialize(deserializer)? {
        Some(value) => EditTitle::Value(value),
        None => EditTitle::Null,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDevPostRequest {
    pub token_id: String,
    pub title: String,
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
    pub poll: Option<CreatePollRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditDevPostRequest {
    #[serde(default, deserialize_with = "deserialize_edit_title")]
    #[schema(value_type = String, required = false, nullable = false)]
    pub title: EditTitle,
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
}
```

Replace validation with:

```rust
impl CreateDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Title must not be blank".into());
        }
        validate_images(&self.image_uris)?;
        validate_poll(&self.poll)
    }
}

impl EditDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        match &self.title {
            EditTitle::Null => return Err("Title must not be null or blank".into()),
            EditTitle::Value(value) if value.trim().is_empty() => {
                return Err("Title must not be null or blank".into());
            }
            EditTitle::Omitted | EditTitle::Value(_) => {}
        }
        if self.title == EditTitle::Omitted && self.body.is_none() && self.image_uris.is_none() {
            return Err("Nothing to update".into());
        }
        validate_images(&self.image_uris)
    }
}
```

Add `pub title: String` immediately before `pub body: String` in `DevPostResponse`, and add explicit `title: "Announcement".into()` to every response constructor in this file.

- [ ] **Step 4: Preserve `400` versus `413` in the repository error envelope**

Add to `AppError` and its `IntoResponse` match in `src/result.rs`:

```rust
PayloadTooLarge(String),
// match arm
AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg),
```

In `src/router/dev_post/handler.rs`, import `axum::extract::rejection::JsonRejection` and add:

```rust
fn map_devpost_json_rejection(rejection: JsonRejection) -> AppError {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::PayloadTooLarge("Request body exceeds the 100000-byte limit".into())
    } else {
        AppError::BadRequest(format!("Invalid request body: {rejection}"))
    }
}
```

Change both protected JSON handler arguments to `payload: Result<Json<...>, JsonRejection>` and extract before validation:

```rust
let Json(mut payload) = payload.map_err(map_devpost_json_rejection)?; // create
let Json(payload) = payload.map_err(map_devpost_json_rejection)?;     // edit
```

Add `(status = 413, description = "Request body exceeds the 100000-byte global limit")` to both create and edit `utoipa::path` response lists. Do not move authentication out of `route_layer`; middleware must still run before either handler/extractor. The exact current global limit is `DefaultBodyLimit::max(100_000)` bytes in `src/main.rs`.

- [ ] **Step 5: Run GREEN and the complete type/error unit suite**

```bash
cargo test types::dev_post -- --nocapture
cargo test result::error_status_tests -- --nocapture
cargo test router::dev_post::handler -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: all tests pass; valid title bytes remain unchanged; error envelope maps body-limit rejection to `413`.

- [ ] **Step 6: Commit the wire contract**

```bash
git add src/types/dev_post/mod.rs src/result.rs src/router/dev_post/handler.rs
git commit -m "feat(dev-post): require explicit title wire contract"
```

### Task 4: Store, edit, and hydrate title/body independently

**Files:**
- Modify: `src/controllers/dev_post/mod.rs`
- Modify: `src/controllers/dev_post/pin_feed_tests.rs`
- Test: controller tests in both files

**Interfaces:**
- Consumes: `CreateDevPostRequest.title`, `EditTitle::value()`, and the merged `dev_post.title` column.
- Produces unchanged method signatures for `create_post`, `edit_post`, `hydrate_base`, `get_feed_base`, `get_post_rw`, and `get_trending_base`, but every `DevPostResponse` now contains required `title` and description-only `body`.

- [ ] **Step 1: Add failing create/edit/read tests with intentional title fixtures**

Add controller tests that use the real migrations and these assertions:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn create_stores_title_and_empty_description_separately(pool: sqlx::PgPool) {
    seed_token(&pool).await;
    let controller = controller(pool.clone());
    let request: CreateDevPostRequest = serde_json::from_str(
        r#"{"token_id":"0x0000000000000000000000000000000000007777","title":"  Multi\nline  "}"#,
    ).unwrap();
    let id = controller.create_post(CREATOR, &request).await.unwrap();
    let stored: (String, String) =
        sqlx::query_as("SELECT title, body FROM dev_post WHERE id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(stored, ("  Multi\nline  ".into(), "".into()));
    let response = controller.get_post_rw(id, Some(CREATOR)).await.unwrap();
    assert_eq!(response.title, "  Multi\nline  ");
    assert_eq!(response.body, "");
}

#[sqlx::test(migrations = "./migrations-test")]
async fn edit_title_tristate_is_atomic_with_body_and_images(pool: sqlx::PgPool) {
    let id = seed_titled_post(&pool, "Original", "description").await;
    let controller = controller(pool.clone());
    let omitted: EditDevPostRequest = serde_json::from_str(r#"{"body":"new body"}"#).unwrap();
    controller.edit_post(id, CREATOR, &omitted).await.unwrap();
    let after_omitted: (String, String) =
        sqlx::query_as("SELECT title, body FROM dev_post WHERE id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(after_omitted, ("Original".into(), "new body".into()));

    let title_only: EditDevPostRequest =
        serde_json::from_str(r#"{"title":"  Replaced\nexactly  "}"#).unwrap();
    controller.edit_post(id, CREATOR, &title_only).await.unwrap();
    let response = controller.get_post_rw(id, Some(CREATOR)).await.unwrap();
    assert_eq!(response.title, "  Replaced\nexactly  ");
    assert_eq!(response.body, "new body");
    assert!(response.is_edited);
}

#[sqlx::test(migrations = "./migrations-test")]
async fn every_read_surface_returns_title_and_description_only_body(pool: sqlx::PgPool) {
    let id = seed_titled_post(&pool, "Headline", "https://x.com/naddotfun/status/1 details").await;
    let controller = controller(pool.clone());
    controller.pin_post(id, CREATOR).await.unwrap();
    let feed = controller.get_feed_base(Some(TOKEN), 1, 20).await.unwrap();
    assert_eq!(feed.pin.as_ref().unwrap().title, "Headline");
    assert_eq!(feed.pin.as_ref().unwrap().body, "https://x.com/naddotfun/status/1 details");
    let detail = controller.get_post_rw(id, None).await.unwrap();
    assert_eq!(detail.title, "Headline");
    assert_eq!(detail.tweet_url.as_deref(), Some("https://x.com/naddotfun/status/1"));
    let trending = controller.get_trending_base().await.unwrap();
    assert!(trending.iter().any(|post| post.id == id.to_string() && post.title == "Headline"));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn edit_title_body_and_images_roll_back_atomically(pool: sqlx::PgPool) {
    let id = seed_titled_post_with_image(&pool, "Original", "description", "old-image").await;
    sqlx::raw_sql(
        "CREATE FUNCTION fail_new_image() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected image failure'; END $$; \
         CREATE TRIGGER fail_new_image BEFORE INSERT ON dev_post_image \
         FOR EACH ROW EXECUTE FUNCTION fail_new_image()",
    ).execute(&pool).await.unwrap();
    let request: EditDevPostRequest = serde_json::from_str(
        r#"{"title":"Replacement","body":"replacement body","image_uris":["new-image"]}"#,
    ).unwrap();
    assert!(matches!(controller(pool.clone()).edit_post(id, CREATOR, &request).await,
        Err(AppError::InternalError(_))));
    let stored: (String, String, String) = sqlx::query_as(
        "SELECT dp.title,dp.body,dpi.image_uri FROM dev_post dp \
         JOIN dev_post_image dpi ON dpi.post_id=dp.id WHERE dp.id=$1",
    ).bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(stored, ("Original".into(), "description".into(), "old-image".into()));
}
```

Define `seed_titled_post(pool, title, body)` with `INSERT INTO dev_post (token_id, author, title, body)` and `seed_titled_post_with_image` as its one-image variant; update every direct fixture in touched files to provide an intentional title. The migration test is the only place that intentionally relies on default-empty legacy title.

- [ ] **Step 2: Run RED**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::tests::create_stores_title -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::tests::edit_title_tristate -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::tests::every_read_surface -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::tests::edit_title_body_and_images_roll_back_atomically -- --nocapture
```

Expected: SQL/constructor failures because production create/edit/hydration does not yet
bind/select title; the rollback test also fails for the missing title-aware atomic edit.

- [ ] **Step 3: Make create and edit SQL title-aware**

Replace create insert with:

```rust
let id: i64 = sqlx::query_scalar(
    "INSERT INTO dev_post (token_id, author, title, body) \
     VALUES ($1,$2,$3,$4) RETURNING id",
)
.bind(&req.token_id)
.bind(author)
.bind(&req.title)
.bind(req.body.as_deref().unwrap_or(""))
.fetch_one(&mut *tx)
.await
.map_err(|error| AppError::InternalError(error.to_string()))?;
```

Replace the edit's body branch with one update that preserves omitted fields:

```rust
let token_id: String = sqlx::query_scalar(
    "UPDATE dev_post \
     SET title = COALESCE($2, title), body = COALESCE($3, body), \
         updated_at = NOW(), edited_at = NOW() \
     WHERE id = $1 RETURNING token_id",
)
.bind(post_id)
.bind(req.title.value())
.bind(req.body.as_deref())
.fetch_one(&mut *tx)
.await
.map_err(|error| AppError::InternalError(error.to_string()))?;
```

Validation has already rejected `EditTitle::Null`; do not map null to preservation inside the controller.

- [ ] **Step 4: Select and map the two content columns directly**

Add `title: String` to `CoreRow`, change the core select to include `dp.title`, and construct:

```rust
DevPostResponse {
    id: id.to_string(),
    token: TokenSummary {
        token_id: core.token_id,
        name: core.token_name,
        symbol: core.token_symbol,
        image_uri: Some(core.token_image),
        market_cap: None,
    },
    author: AuthorSummary {
        account_id: core.author,
        nickname: core.author_nickname,
        image_uri: core.author_image,
    },
    title: core.title,
    tweet_url: parse_tweet_url(&core.body),
    body: core.body,
    images: images_by_post.remove(&id).unwrap_or_default(),
    poll,
    like_count: *like_counts.get(&id).unwrap_or(&0),
    liked_by_me: false,
    is_edited: core.edited_at.is_some(),
    created_at: core.created_at,
    updated_at: core.updated_at,
}
```

There is no runtime split, concatenation, default title synthesis, or title-based tweet parsing.

- [ ] **Step 5: Run the focused atomic-edit GREEN check**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::tests::edit_title_body_and_images_roll_back_atomically \
  -- --nocapture
```

Expected: the trigger failure leaves the original title, body, and image unchanged.

- [ ] **Step 6: Run GREEN and all Dev Post controller/pin tests**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::pin_feed_tests -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: all controller/pin/feed/detail/trending/read-after-write tests pass with explicit title fixtures.

- [ ] **Step 7: Commit independent storage/hydration**

```bash
git add src/controllers/dev_post/mod.rs src/controllers/dev_post/pin_feed_tests.rs
git commit -m "feat(dev-post): store and hydrate title separately"
```

### Task 5: Cut payload caches to v3/v2 and capture ranking generation

**Files:**
- Modify: `src/config.rs`
- Modify: `src/db/redis/mod.rs`
- Modify: `src/services/dev_post/mod.rs`
- Modify: `src/main.rs`
- Test: Redis/service unit and integration tests in those modules

**Interfaces:**
- Produces exact key helpers for feed v3, detail v2, trending v2, generation, and generation-scoped ranking.
- Produces `get_devpost_ranking_generation() -> anyhow::Result<i64>`, `bump_devpost_ranking_generation() -> anyhow::Result<i64>`, and generation-parameterized ranking get/set.
- Produces exact multi-version delete methods; later moderation uses the same helpers.

#### Step 1 reference: Cache namespace, generation, invalidation, and TTL tests

Under the existing Redis test module, add tests that use the process prefix and assert:

```rust
#[tokio::test]
async fn title_payload_namespaces_ignore_old_shapes_and_delete_all_exact_versions() {
    assert_test_redis_namespace();
    let redis = RedisDatabase::new().await;
    let scope = format!("scope-{}", uuid::Uuid::new_v4().simple());
    let post_id = 5101_i64;
    raw_set(&devpost_feed_v2_key(&scope), r#"{"posts":[],"total_count":99}"#).await;
    raw_set(&devpost_detail_legacy_key(post_id), r#"{"body":"old combined"}"#).await;
    raw_set(&devpost_trending_legacy_key(), r#"[{"body":"old combined"}]"#).await;
    assert!(redis.get_devpost_feed_base(&scope).await.is_err());
    assert!(redis.get_devpost_detail_base(post_id).await.is_err());
    assert!(redis.get_devpost_trending_base().await.is_err());

    seed_all_feed_versions(&scope).await;
    seed_all_detail_versions(post_id).await;
    seed_all_trending_versions().await;
    redis.delete_devpost_feed(&scope).await.unwrap();
    redis.delete_devpost_detail(post_id).await.unwrap();
    redis.delete_devpost_trending().await.unwrap();
    assert_all_exact_payload_versions_absent(&scope, post_id).await;
}

#[tokio::test]
async fn generation_missing_is_zero_and_concurrent_bumps_are_not_lost() {
    assert_test_redis_namespace();
    let redis = std::sync::Arc::new(RedisDatabase::new().await);
    delete_exact_generation_key().await;
    assert_eq!(redis.get_devpost_ranking_generation().await.unwrap(), 0);
    let (a, b) = tokio::join!(
        redis.bump_devpost_ranking_generation(),
        redis.bump_devpost_ranking_generation(),
    );
    let mut values = vec![a.unwrap(), b.unwrap()];
    values.sort_unstable();
    assert_eq!(values, vec![1, 2]);
}
```

The helper assertions enumerate only the six statically known payload keys supplied by the test; they never discover keys.

Before writing service/config implementation, also add these named tests:

- `create_uses_full_payload_invalidation_and_bumps_generation` seeds every exact old/new
  create key and asserts generation increments once;
- `edit_uses_full_payload_invalidation_without_generation_bump` seeds every exact
  old/new edit key and asserts generation is unchanged;
- `devpost_cache_ttl_ceiling_accepts_60000_and_rejects_60001` calls the pure wished-for
  helper and asserts the failing variable name is present.

- [ ] **Step 1a: Add the failing payload-namespace test**

Add `title_payload_namespaces_ignore_old_shapes_and_delete_all_exact_versions`, then run:

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-cache-namespace-red-$(uuidgen):" \
  cargo test db::redis::devpost::title_payload_namespaces_ignore_old_shapes \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1b: Add the failing ranking-generation test**

Add `generation_missing_is_zero_and_concurrent_bumps_are_not_lost`, then run:

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-cache-generation-red-$(uuidgen):" \
  cargo test db::redis::devpost::generation_missing_is_zero_and_concurrent_bumps_are_not_lost \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1c: Add the failing create-invalidation test**

Add `create_uses_full_payload_invalidation_and_bumps_generation`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-create-invalidation-red-$(uuidgen):" cargo test \
  services::dev_post::tests::create_uses_full_payload_invalidation_and_bumps_generation \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1d: Add the failing edit-invalidation test**

Add `edit_uses_full_payload_invalidation_without_generation_bump`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-edit-invalidation-red-$(uuidgen):" cargo test \
  services::dev_post::tests::edit_uses_full_payload_invalidation_without_generation_bump \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1e: Add the failing TTL-boundary test**

Add `devpost_cache_ttl_ceiling_accepts_60000_and_rejects_60001`, then run
`cargo test config::tests::devpost_cache_ttl_ceiling_accepts_60000_and_rejects_60001 -- --nocapture`.

- [ ] **Step 2: Run RED in a disposable Redis namespace**

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-dev-post-title-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test db::redis::devpost -- --nocapture --test-threads=1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-dev-post-title-red-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test services::dev_post::tests::create_uses_full_payload_invalidation -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-dev-post-title-red-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test services::dev_post::tests::edit_uses_full_payload_invalidation -- --nocapture
cargo test config::tests::devpost_cache_ttl_ceiling -- --nocapture
```

Expected: compile failures for missing v3/v2/generation/invalidation helpers and the TTL
validator.

- [ ] **Step 3: Implement exact new read/write keys and multi-version deletion**

Add these key builders in `src/db/redis/mod.rs`:

```rust
fn devpost_feed_v3_key(scope: &str) -> String {
    with_prefix(format!("devpost:feed:v3:{scope}"))
}
fn devpost_detail_legacy_key(post_id: i64) -> String {
    with_prefix(format!("devpost:detail:{post_id}"))
}
fn devpost_detail_v2_key(post_id: i64) -> String {
    with_prefix(format!("devpost:detail:v2:{post_id}"))
}
fn devpost_trending_legacy_key() -> String {
    with_prefix("devpost:trending".into())
}
fn devpost_trending_v2_key() -> String {
    with_prefix("devpost:trending:v2".into())
}
fn devpost_ranking_generation_key() -> String {
    with_prefix("devpost:ranking:generation".into())
}
fn devpost_ranking_v2_key(generation: i64, page: i64, limit: i64) -> String {
    with_prefix(format!("devpost:ranking:v2:{generation}:{page}:{limit}"))
}
```

Feed/detail/trending get/set methods use only v3/v2 keys. Delete methods use one atomic pipeline per family to delete feed legacy/v2/v3, detail legacy/v2, and trending legacy/v2. No read method falls back after deserialization failure.

- [ ] **Step 4: Implement captured-generation Redis primitives**

```rust
pub async fn get_devpost_ranking_generation(&self) -> Result<i64> {
    let mut conn = self.conn.as_ref().clone();
    let value: Option<i64> = measure_redis!(
        "redis.get_devpost_ranking_generation",
        conn.get(devpost_ranking_generation_key())
    )?;
    Ok(value.unwrap_or(0))
}

pub async fn bump_devpost_ranking_generation(&self) -> Result<i64> {
    let mut conn = self.conn.as_ref().clone();
    Ok(measure_redis!(
        "redis.bump_devpost_ranking_generation",
        conn.incr(devpost_ranking_generation_key(), 1_i64)
    )?)
}

pub async fn get_devpost_ranking_response_for_generation(
    &self, generation: i64, page: i64, limit: i64,
) -> Result<RankingResponse> {
    let mut conn = self.conn.as_ref().clone();
    let json: String = conn.get(devpost_ranking_v2_key(generation, page, limit)).await?;
    Ok(serde_json::from_str(&json)?)
}

pub async fn set_devpost_ranking_response_for_generation(
    &self, generation: i64, page: i64, limit: i64, response: &RankingResponse,
) -> Result<()> {
    let mut conn = self.conn.as_ref().clone();
    let json = serde_json::to_string(response)?;
    conn.pset_ex::<_, _, ()>(
        devpost_ranking_v2_key(generation, page, limit),
        json,
        *DEVPOST_RANKING_EXPIRATION,
    ).await?;
    Ok(())
}
```

Remove production callers of legacy `devpost:ranking:{page}:{limit}` methods.

- [ ] **Step 5: Capture generation exactly once in service ranking and apply write policy**

Replace ranking cache-aside with:

```rust
let generation = match self.redis.get_devpost_ranking_generation().await {
    Ok(value) => value,
    Err(error) => {
        tracing::warn!(event = "dev_post.ranking_generation_read_failed", error = %error);
        return DevPostController::new(self.postgres.clone()).get_ranking(page, limit).await;
    }
};
if let Ok(cached) = self.redis
    .get_devpost_ranking_response_for_generation(generation, page, limit).await
{
    return Ok((cached.rankings, cached.total_count));
}
let (rankings, total_count) = DevPostController::new(self.postgres.clone())
    .get_ranking(page, limit).await?;
let response = RankingResponse { rankings, total_count };
let _ = self.redis
    .set_devpost_ranking_response_for_generation(generation, page, limit, &response).await;
Ok((response.rankings, response.total_count))
```

After successful create, delete global/token feed and both trending keys and independently bump generation. After successful edit, delete global/token feed, detail, and trending but do not bump. Pin/unpin deletes token feed versions only.

Use these minimal helpers so the policy is named and testable:

```rust
async fn invalidate_after_create(&self, token_id: &str) {
    let _ = tokio::join!(
        self.redis.delete_devpost_feed("global"),
        self.redis.delete_devpost_feed(token_id),
        self.redis.delete_devpost_trending(),
        self.redis.bump_devpost_ranking_generation(),
    );
}

async fn invalidate_after_edit(&self, post_id: i64, token_id: &str) {
    let _ = tokio::join!(
        self.redis.delete_devpost_feed("global"),
        self.redis.delete_devpost_feed(token_id),
        self.redis.delete_devpost_detail(post_id),
        self.redis.delete_devpost_trending(),
    );
}
```

Implement the already-RED named invalidation tests from Step 1 by making the helpers
visible to their in-module tests; do not add new behavior after their GREEN run.

- [ ] **Step 6: Enforce the four TTL ceiling at startup**

Add a pure helper and startup wrapper in `src/config.rs`:

```rust
pub fn validate_devpost_cache_ttls(values: [(&str, u64); 4]) -> Result<(), String> {
    for (name, value) in values {
        if value > 60_000 {
            return Err(format!("{name} must be <= 60000ms, got {value}"));
        }
    }
    Ok(())
}

pub fn validate_current_devpost_cache_ttls() -> Result<(), String> {
    validate_devpost_cache_ttls([
        ("DEVPOST_FEED_EXPIRATION", *DEVPOST_FEED_EXPIRATION),
        ("DEVPOST_DETAIL_EXPIRATION", *DEVPOST_DETAIL_EXPIRATION),
        ("DEVPOST_TRENDING_EXPIRATION", *DEVPOST_TRENDING_EXPIRATION),
        ("DEVPOST_RANKING_EXPIRATION", *DEVPOST_RANKING_EXPIRATION),
    ])
}
```

Call `config::validate_current_devpost_cache_ttls().expect("invalid Dev Post cache TTL configuration")` before building the router in `src/main.rs`. Make the already-RED boundary test from Step 1 pass: exactly `60_000` succeeds and `60_001` fails naming the offending variable.

- [ ] **Step 7: Run GREEN and source-only cache safety scans**

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-dev-post-title-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test db::redis::devpost -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-dev-post-title-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test services::dev_post -- --nocapture --test-threads=1
cargo test config:: -- --nocapture
rg -n 'devpost:ranking:\{\}|devpost:ranking:\{}' src/db/redis/mod.rs src/services/dev_post/mod.rs && exit 1 || true
if git diff --unified=0 HEAD^ -- src/db/redis/mod.rs src/services/dev_post/mod.rs | rg '^\+.*(KEYS|SCAN|FLUSHALL)'; then exit 1; fi
cargo fmt --all -- --check
git diff --check
```

Expected: new namespaces/generation tests pass; changed source contains no legacy ranking access or Redis enumeration.

- [ ] **Step 8: Commit the cache cutover**

```bash
git add src/config.rs src/main.rs src/db/redis/mod.rs src/services/dev_post/mod.rs
git commit -m "feat(dev-post): version title caches and ranking generation"
```

### Task 6: Implement the shared author/admin moderation transaction core

**Files:**
- Create: `src/controllers/dev_post/moderation.rs`
- Create: `src/controllers/dev_post/moderation_tests.rs`
- Modify: `src/controllers/dev_post/mod.rs`
- Read/retain: `src/controllers/dev_post/pin.rs`

**Interfaces:**
- Produces `CommitOutcome<T>`, `PostMutationContext`, `ModerationAction`, `ModerationContext`, and `ModerationAudit::matches`.
- Produces these exact controller signatures:

```rust
pub(crate) async fn delete_post_as_author(
    &self, post_id: i64, author: &str,
) -> Result<CommitOutcome<PostMutationContext>, AppError>;
pub(crate) async fn delete_post_as_admin(
    &self, post_id: i64, admin: &str, audit_id: uuid::Uuid,
) -> Result<CommitOutcome<ModerationContext>, AppError>;
pub(crate) async fn restore_post_as_admin(
    &self, post_id: i64, admin: &str, audit_id: uuid::Uuid,
) -> Result<CommitOutcome<ModerationContext>, AppError>;
pub(crate) async fn get_moderation_audit_on_write_pool(
    &self, audit_id: uuid::Uuid,
) -> Result<Option<ModerationAudit>, AppError>;
```

#### Step 1 reference: Moderation policy tests

Register `mod moderation; #[cfg(test)] mod moderation_tests;` and add title-aware fixtures using:

```rust
const ADMIN: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
const ADMIN_CASE_MUTATED: &str = "0x52908400098527886e0f7030069857d2e4169ee7";
const OTHER: &str = "0xde709f2102306220921060314715629080e2fb77";
const TOKEN: &str = "0x0000000000000000000000000000000000006000";

async fn seed_post(pool: &sqlx::PgPool) -> i64 {
    seed_token(pool, TOKEN, ADMIN).await;
    sqlx::query_scalar(
        "INSERT INTO dev_post (token_id,author,title,body) \
         VALUES ($1,$2,'Moderation title','Moderation body') RETURNING id",
    ).bind(TOKEN).bind(ADMIN).fetch_one(pool).await.unwrap()
}
```

Add these named tests with direct assertions:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn author_delete_is_non_idempotent_unpins_and_never_audits(pool: sqlx::PgPool) {
    let post_id = seed_post(&pool).await;
    seed_pin(&pool, post_id).await;
    let first = committed(controller(pool.clone())
        .delete_post_as_author(post_id, ADMIN).await.unwrap());
    assert!(first.changed);
    assert_eq!(count_pins(&pool, post_id).await, 0);
    assert_eq!(count_audits(&pool, post_id).await, 0);
    assert!(matches!(controller(pool).delete_post_as_author(post_id, ADMIN).await,
        Err(AppError::NotFound(_))));
}

#[sqlx::test(migrations = "./migrations-test")]
async fn cms_admin_privacy_idempotency_and_orphan_policy_are_exact(pool: sqlx::PgPool) {
    let post_id = seed_post(&pool).await;
    seed_admin(&pool, ADMIN).await;
    for target in [post_id, i64::MAX] {
        assert!(matches!(controller(pool.clone())
            .delete_post_as_admin(target, OTHER, uuid::Uuid::new_v4()).await,
            Err(AppError::Forbidden(_))));
    }
    assert!(matches!(controller(pool.clone())
        .delete_post_as_admin(post_id, ADMIN_CASE_MUTATED, uuid::Uuid::new_v4()).await,
        Err(AppError::Forbidden(_))));
    let deleted = committed(controller(pool.clone())
        .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4()).await.unwrap());
    let repeated = committed(controller(pool.clone())
        .delete_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4()).await.unwrap());
    assert_eq!((deleted.changed, repeated.changed), (true, false));
    sqlx::query("DELETE FROM token WHERE token_id=$1").bind(TOKEN)
        .execute(&pool).await.unwrap();
    assert!(matches!(controller(pool)
        .restore_post_as_admin(post_id, ADMIN, uuid::Uuid::new_v4()).await,
        Err(AppError::Conflict(_))));
}

#[test]
fn failed_commit_retains_exact_pending_context() {
    let context = PostMutationContext { post_id: 9, token_id: TOKEN.into(), changed: true };
    let outcome = classify_commit(context.clone(),
        Err(sqlx::Error::Protocol("lost COMMIT acknowledgement".into())));
    assert!(matches!(outcome,
        CommitOutcome::Unknown { context: retained, error }
        if retained == context && error.contains("lost COMMIT acknowledgement")));
}
```

Define `controller`, `committed`, `seed_admin`, `seed_pin`, `count_pins`, and `count_audits` in the same test file with direct SQL and the exact signatures used above.

- [ ] **Step 1a: Register the module and title-aware fixtures**

Add only the module declarations, constants, `seed_post`, and common helpers; run
`cargo fmt --all -- --check`.

- [ ] **Step 1b: Add the failing author-delete policy test**

Add `author_delete_is_non_idempotent_unpins_and_never_audits`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::author_delete_is_non_idempotent_unpins_and_never_audits \
  -- --nocapture --test-threads=1
```

Expected RED: the author controller method is missing.

- [ ] **Step 1c: Add the failing CMS privacy/idempotency/orphan test**

Add `cms_admin_privacy_idempotency_and_orphan_policy_are_exact`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::cms_admin_privacy_idempotency_and_orphan_policy_are_exact \
  -- --nocapture --test-threads=1
```

Expected RED: the admin delete/restore methods are missing.

- [ ] **Step 1d: Add the failing commit-context test**

Add `failed_commit_retains_exact_pending_context`, then run
`cargo test controllers::dev_post::moderation_tests::failed_commit_retains_exact_pending_context -- --nocapture`.
Expected RED: the context/outcome types are missing.

- [ ] **Step 2: Run RED**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests -- --nocapture
```

Expected: compile failure naming missing moderation types/methods.

- [ ] **Step 3: Define immutable contexts and shared delete**

Create `moderation.rs` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostMutationContext { pub post_id: i64, pub token_id: String, pub changed: bool }

#[derive(Debug)]
pub(crate) enum CommitOutcome<T> {
    Committed(T),
    Unknown { context: T, error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModerationAction { Delete, Restore }
impl ModerationAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self { Self::Delete => "DELETE", Self::Restore => "RESTORE" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModerationContext {
    pub audit_id: uuid::Uuid,
    pub admin_account_id: String,
    pub post_id: i64,
    pub token_id: String,
    pub action: ModerationAction,
    pub changed: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ModerationAudit {
    pub id: uuid::Uuid, pub post_id: i64, pub token_id: String,
    pub admin_account_id: String, pub action: String, pub changed: bool,
}
impl ModerationAudit {
    pub(crate) fn matches(&self, expected: &ModerationContext) -> bool {
        self.id == expected.audit_id && self.post_id == expected.post_id
            && self.token_id == expected.token_id
            && self.admin_account_id == expected.admin_account_id
            && self.action == expected.action.as_str() && self.changed == expected.changed
    }
}

fn classify_commit<T>(context: T, result: Result<(), sqlx::Error>) -> CommitOutcome<T> {
    match result {
        Ok(()) => CommitOutcome::Committed(context),
        Err(error) => CommitOutcome::Unknown { context, error: error.to_string() },
    }
}

async fn finish_transaction<T>(
    tx: sqlx::Transaction<'_, sqlx::Postgres>, context: T,
) -> CommitOutcome<T> {
    classify_commit(context, tx.commit().await)
}
```

Implement `apply_soft_delete(tx: &mut Transaction<'_, Postgres>, post_id: i64) -> Result<bool, AppError>` by deleting `dev_post_pin`, conditionally updating `deleted_at = clock_timestamp()`, and returning `rows_affected()==1`. Each public transaction method wraps all pre-commit work in a `pending` result; on `Err(error)` it executes `let _ = tx.rollback().await; return Err(error);`. Only an attempted `COMMIT` reaches `finish_transaction`, so commit failure retains context as `Unknown` instead of being mislabeled rollback.

- [ ] **Step 4: Implement admin-first DELETE/RESTORE and audit lookup**

Inside one write transaction execute in this order:

```rust
sqlx::query_scalar::<_, String>(
    "SELECT account_id FROM admin WHERE account_id=$1 FOR KEY SHARE"
).bind(admin).fetch_optional(&mut *tx).await?;
sqlx::query_scalar::<_, String>("SELECT token_id FROM dev_post WHERE id=$1")
    .bind(post_id).fetch_optional(&mut *tx).await?;
sqlx::query_scalar::<_, String>(
    "SELECT token_id FROM token WHERE token_id=$1 FOR KEY SHARE"
).bind(&token_id).fetch_optional(&mut *tx).await?;
sqlx::query_as::<_, (bool,)>(
    "SELECT deleted_at IS NOT NULL FROM dev_post \
     WHERE id=$1 AND token_id=$2 FOR UPDATE"
).bind(post_id).bind(&token_id).fetch_optional(&mut *tx).await?;
```

No admin row is `Forbidden`; no post is `NotFound`; missing token is allowed only for DELETE and is `Conflict` for RESTORE. DELETE always invokes shared pin cleanup. RESTORE deletes a stale pin and sets `deleted_at=NULL` only when the locked row was deleted; a live row does neither. Insert the pre-generated UUID and copied context in `dev_post_moderation_log`, then commit through `CommitOutcome`.

Implement audit reconciliation lookup exactly on `self.db.get_write_pool()`:

```sql
SELECT id, post_id, token_id, admin_account_id, action, changed
FROM dev_post_moderation_log WHERE id = $1
```

- [ ] **Step 5: Run GREEN, existing author/pin regressions, and exact-address scan**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::pin -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::pin_feed_tests -- --nocapture
rg -n 'LOWER\(|lower\(|ILIKE' src/controllers/dev_post/moderation.rs && exit 1 || true
cargo fmt --all -- --check
git diff --check
```

Expected: all policy/idempotency/orphan/title-preservation tests pass; scan is empty.

- [ ] **Step 6: Commit the transaction core**

```bash
git add src/controllers/dev_post/mod.rs src/controllers/dev_post/moderation.rs \
  src/controllers/dev_post/moderation_tests.rs
git commit -m "feat(dev-post): add audited moderation transaction core"
```

### Task 7: Reconcile commit outcomes and invalidate every public representation

**Files:**
- Modify: `src/services/dev_post/mod.rs`
- Create: `src/services/dev_post/moderation_tests.rs`
- Modify: `src/controllers/dev_post/moderation.rs`

**Interfaces:**
- Produces `DevPostService::{delete_post_as_admin,restore_post_as_admin}` and updates author `delete_post` to consume `CommitOutcome`.
- Produces these exact reconciliation/injection signatures:

```rust
async fn finish_author_delete(
    &self, outcome: CommitOutcome<PostMutationContext>,
) -> Result<(), AppError>;
async fn finish_cms_moderation(
    &self, controller: &DevPostController, outcome: CommitOutcome<ModerationContext>,
) -> Result<(), AppError>;
async fn finish_cms_unknown_after_lookup(
    &self,
    context: ModerationContext,
    commit_error: String,
    lookup: Result<Option<ModerationAudit>, AppError>,
) -> Result<(), AppError>;
async fn run_invalidation_futures(
    &self, post_id: i64, token_id: &str, futures: InvalidationFutures<'_>,
) -> InvalidationReport;
```

#### Step 1 reference: Reconciliation and invalidation tests

Register the test file from `src/services/dev_post/mod.rs` with `#[cfg(test)] mod moderation_tests;`.

The matching fixture is:

```rust
let context = ModerationContext {
    audit_id: uuid::Uuid::new_v4(), admin_account_id: ADMIN.into(), post_id: 81,
    token_id: TOKEN.into(), action: ModerationAction::Delete, changed: true,
};
let audit = ModerationAudit {
    id: context.audit_id, post_id: context.post_id, token_id: context.token_id.clone(),
    admin_account_id: context.admin_account_id.clone(), action: "DELETE".into(), changed: true,
};
assert!(audit.matches(&context));
```

Add the named tests and exact injected outcomes:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn exact_audit_match_reconciles_commit_error(pool: sqlx::PgPool) {
    let service = service(pool.clone(), test_redis().await);
    let context = cms_context(ModerationAction::Delete, true);
    insert_matching_audit(&pool, &context).await;
    let controller = DevPostController::new(service.postgres.clone());
    let result = service.finish_cms_moderation(&controller, CommitOutcome::Unknown {
        context, error: "lost COMMIT acknowledgement".into(),
    }).await;
    assert!(result.is_ok());
}

#[sqlx::test(migrations = "./migrations-test")]
async fn missing_mismatched_and_failed_reconciliation_are_outcome_unknown(pool: sqlx::PgPool) {
    let service = service(pool, test_redis().await);
    let context = cms_context(ModerationAction::Restore, false);
    let mut mismatch = matching_audit(&context);
    mismatch.action = "DELETE".into();
    for lookup in [
        Ok(None), Ok(Some(mismatch)),
        Err(AppError::InternalError("write pool unavailable".into())),
    ] {
        let error = service.finish_cms_unknown_after_lookup(
            context.clone(), "lost COMMIT acknowledgement".into(), lookup,
        ).await.unwrap_err();
        assert!(matches!(error, AppError::InternalError(message)
            if message == "outcome_unknown"));
    }
}

#[tokio::test]
async fn one_redis_failure_does_not_skip_later_families() {
    let calls = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let futures = injected_invalidation_futures(calls.clone(), Some("global_feed"));
    let report = service_without_database(test_redis().await)
        .run_invalidation_futures(81, TOKEN, futures).await;
    let mut attempted = calls.lock().await.clone();
    attempted.sort_unstable();
    assert_eq!(attempted,
        ["detail", "global_feed", "ranking_generation", "token_feed", "trending"]);
    assert_eq!(report.failed_families, vec!["global_feed"]);
}
```

`cms_context`, `matching_audit`, `insert_matching_audit`, `test_redis`, `service`, and `service_without_database` are defined in the same test file with the exact types used here. The last helper's Postgres pools are lazy and never accessed by `run_invalidation_futures`.

- [ ] **Step 1a: Register the test module and bounded fixtures/helpers**

Register `moderation_tests`, then add only `cms_context`, `matching_audit`,
`insert_matching_audit`, `test_redis`, `service`, and `service_without_database`. Run
`cargo fmt --all -- --check` before adding a test.

- [ ] **Step 1b: Add and run the exact-audit reconciliation RED**

Add only `exact_audit_match_reconciles_commit_error`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-task7-audit-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::exact_audit_match_reconciles_commit_error \
  -- --nocapture --test-threads=1
```

Expected RED: the service reconciliation entry point is missing.

- [ ] **Step 1c: Add and run the unknown-outcome reconciliation RED**

Add only `missing_mismatched_and_failed_reconciliation_are_outcome_unknown`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-task7-unknown-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::missing_mismatched_and_failed_reconciliation_are_outcome_unknown \
  -- --nocapture --test-threads=1
```

Expected RED: the injected lookup policy is missing.

- [ ] **Step 1d: Add and run the Redis failure-isolation RED**

Add only `one_redis_failure_does_not_skip_later_families`, then run:

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-task7-isolation-red-$(uuidgen):" \
  cargo test services::dev_post::moderation_tests::one_redis_failure_does_not_skip_later_families \
  -- --nocapture --test-threads=1
```

Expected RED: `InvalidationFutures`, `InvalidationReport`, and the runner are missing.

- [ ] **Step 2: Run RED in isolated Redis**

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-moderation-title-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test services::dev_post::moderation_tests -- --nocapture --test-threads=1
```

Expected: compile failures for service CMS/reconciliation helpers.

- [ ] **Step 3: Add service entry points and exact commit-boundary policy**

```rust
pub async fn delete_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError> {
    if post_id <= 0 { return Err(AppError::BadRequest("post_id must be positive".into())); }
    let controller = DevPostController::new(self.postgres.clone());
    let outcome = controller
        .delete_post_as_admin(post_id, admin, uuid::Uuid::new_v4()).await?;
    self.finish_cms_moderation(&controller, outcome).await
}

pub async fn restore_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError> {
    if post_id <= 0 { return Err(AppError::BadRequest("post_id must be positive".into())); }
    let controller = DevPostController::new(self.postgres.clone());
    let outcome = controller
        .restore_post_as_admin(post_id, admin, uuid::Uuid::new_v4()).await?;
    self.finish_cms_moderation(&controller, outcome).await
}
```

For committed CMS context, invalidate then emit `event="cms.dev_post.moderation"`. For unknown, query the audit on the write pool; only `Some(audit) if audit.matches(&context)` becomes reconciled `204`. Every other lookup outcome invalidates, emits `event="cms.dev_post.moderation.outcome_unknown"`, and returns `InternalError("outcome_unknown")`. Author unknown invalidates, logs `dev_post.author_delete.outcome_unknown`, and returns the same classification without audit lookup.

Implement `finish_cms_unknown_after_lookup` with the exact injected `lookup` parameter above; production `finish_cms_moderation` supplies `controller.get_moderation_audit_on_write_pool(context.audit_id).await`. This is the audit-query failure seam and prevents tests from changing production database routing.

#### Step 4 reference: Independent best-effort full invalidation

```rust
type UnitInvalidation<'a> = std::pin::Pin<Box<
    dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a,
>>;
type RankingInvalidation<'a> = std::pin::Pin<Box<
    dyn std::future::Future<Output = anyhow::Result<i64>> + Send + 'a,
>>;

struct InvalidationFutures<'a> {
    global_feed: UnitInvalidation<'a>,
    token_feed: UnitInvalidation<'a>,
    detail: UnitInvalidation<'a>,
    trending: UnitInvalidation<'a>,
    ranking_generation: RankingInvalidation<'a>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct InvalidationReport { failed_families: Vec<&'static str> }

async fn invalidate_post_public_caches(&self, post_id: i64, token_id: &str) {
    let futures = InvalidationFutures {
        global_feed: Box::pin(self.redis.delete_devpost_feed("global")),
        token_feed: Box::pin(self.redis.delete_devpost_feed(token_id)),
        detail: Box::pin(self.redis.delete_devpost_detail(post_id)),
        trending: Box::pin(self.redis.delete_devpost_trending()),
        ranking_generation: Box::pin(self.redis.bump_devpost_ranking_generation()),
    };
    let _ = self.run_invalidation_futures(post_id, token_id, futures).await;
}

async fn run_invalidation_futures(
    &self, post_id: i64, token_id: &str, futures: InvalidationFutures<'_>,
) -> InvalidationReport {
    let (global, token, detail, trending, ranking) = tokio::join!(
        futures.global_feed, futures.token_feed, futures.detail,
        futures.trending, futures.ranking_generation,
    );
    let mut report = InvalidationReport::default();
    for (family, result) in [
        ("global_feed", global), ("token_feed", token),
        ("detail", detail), ("trending", trending),
    ] {
        if let Err(error) = result {
            report.failed_families.push(family);
            tracing::warn!(event="dev_post.cache_invalidation_failed", family,
                post_id, token_id, error=%error);
        }
    }
    if let Err(error) = ranking {
        report.failed_families.push("ranking_generation");
        tracing::warn!(event="dev_post.cache_invalidation_failed",
            family="ranking_generation", post_id, token_id, error=%error);
    }
    report
}
```

In `moderation_tests.rs`, implement `injected_invalidation_futures(calls, fail_family)` by returning five `Box::pin(async move { calls.lock().await.push("family"); Ok/Err })` futures; ranking returns `Ok(1)` and unit families return `Ok(())` unless their exact name equals `fail_family`. No warning/info/error field receives title or body.

- [ ] **Step 4a: Define the invalidation future/report types**

Add only `UnitInvalidation`, `RankingInvalidation`, `InvalidationFutures`, and
`InvalidationReport` from the reference; run `cargo fmt --all -- --check`.

- [ ] **Step 4b: Build the five exact public-cache futures**

Add only `invalidate_post_public_caches`, wiring global feed, token feed, detail,
trending, and ranking generation once each; run `cargo fmt --all -- --check`.

- [ ] **Step 4c: Implement the independent runner and make failure isolation GREEN**

Add `run_invalidation_futures` plus `injected_invalidation_futures`, then run:

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-task7-isolation-green-$(uuidgen):" \
  cargo test services::dev_post::moderation_tests::one_redis_failure_does_not_skip_later_families \
  -- --nocapture --test-threads=1
```

Expected GREEN: all five exact families are attempted and only the injected family is
reported failed.

- [ ] **Step 4d: Wire invalidation into commit reconciliation and run GREEN**

Call the completed invalidation helper from committed, reconciled, and unknown author/CMS
outcomes, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-task7-reconciliation-green-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::exact_audit_match_reconciles_commit_error \
  -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-task7-unknown-green-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::missing_mismatched_and_failed_reconciliation_are_outcome_unknown \
  -- --nocapture --test-threads=1
```

Both must pass before Step 5.

- [ ] **Step 5: Run GREEN and retry both possible unknown outcomes**

Add `retries_are_safe_for_committed_and_uncommitted_unknown_outcomes`: one actual committed DELETE is deliberately fed to `finish_cms_unknown_after_lookup(..., Ok(None))` then retried (new audit is `changed=false`); one imaginary uncommitted context is fed to the same seam then a real retry changes state (`changed=true`). Assert two `changed=true` and one `changed=false` across the two posts. Run:

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-moderation-title-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test services::dev_post::moderation_tests -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: all reconciliation, retry, no-op repair, failure-isolation, and content-free log tests pass.

- [ ] **Step 6: Commit orchestration**

```bash
git add src/controllers/dev_post/moderation.rs src/services/dev_post/mod.rs \
  src/services/dev_post/moderation_tests.rs
git commit -m "feat(dev-post): reconcile moderation and invalidate caches"
```

### Task 8: Expose authenticated CMS routes and prove title/auth extraction precedence

**Files:**
- Modify: `src/router/cms/path.rs`
- Modify: `src/router/cms/handler.rs`
- Modify: `src/router/cms/mod.rs`
- Modify: `src/router/dev_post/handler.rs`
- Modify: `src/router/dev_post/mod.rs`

**Interfaces:**
- Produces runtime `/cms/dev-post/:post_id` and `/cms/dev-post/:post_id/restore`; docs paths use braces.
- Produces byte-empty `204` handlers with no request schema/body.

- [ ] **Step 1: Add failing real-router tests**

Add authenticated/unauthenticated requests asserting:

```rust
assert_status(Method::DELETE, "/cms/dev-post/not-an-int", None, 401).await;
assert_status(Method::DELETE, "/cms/dev-post/not-an-int", Some(NON_ADMIN_SESSION), 400).await;
assert_status(Method::DELETE, "/cms/dev-post/1", Some(NON_ADMIN_SESSION), 403).await;
assert_status(Method::DELETE, "/cms/dev-post/0", Some(ADMIN_SESSION), 400).await;
assert_status(Method::POST, "/cms/dev-post/999999/restore", Some(ADMIN_SESSION), 404).await;
let response = request(Method::DELETE, live_post_path, Some(ADMIN_SESSION), Body::empty()).await;
assert_eq!(response.status(), StatusCode::NO_CONTENT);
assert!(to_bytes(response.into_body(), usize::MAX).await.unwrap().is_empty());
```

For Dev Post create/edit, test unauthenticated missing/null/blank/malformed/over-limit bodies are `401`; authenticated missing/null/blank/malformed are JSON-envelope `400`; authenticated bodies over the exact `100_000`-byte global limit are JSON-envelope `413`; valid internal-newline title round-trips and a long title below `100_000` total request bytes succeeds.

- [ ] **Step 2: Run RED**

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-router-$(uuidgen):" \
  cargo test router::cms -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-router-$(uuidgen):" \
  cargo test router::dev_post -- --nocapture --test-threads=1
```

Expected: CMS routes are missing and title precedence assertions fail before routing changes.

- [ ] **Step 3: Add distinct runtime/docs paths and thin handlers**

Add `DeleteDevPost` and `RestoreDevPost` variants. `as_str()` returns `/cms/dev-post/:post_id` and `/cms/dev-post/:post_id/restore`; `docs_str()` returns `/cms/dev-post/{post_id}` and `/cms/dev-post/{post_id}/restore`. Implement these annotations and handlers; neither annotation has `request_body`:

```rust
#[utoipa::path(
    delete,
    path = CmsPath::DeleteDevPost.docs_str(),
    params(("post_id" = i64, Path, description = "Positive Dev Post BIGINT ID")),
    responses(
        (status = 204, description = "Deleted or already deleted; empty body"),
        (status = 400, description = "Malformed or nonpositive post_id"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Administrator access required"),
        (status = 404, description = "Physical post row not found"),
        (status = 500, description = "Database failure or outcome_unknown")
    ),
    tag = "CMS"
)]
pub async fn delete_dev_post(
    State(state): State<AppState>, Extension(admin): Extension<String>, Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .delete_post_as_admin(post_id, &admin).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = CmsPath::RestoreDevPost.docs_str(),
    params(("post_id" = i64, Path, description = "Positive Dev Post BIGINT ID")),
    responses(
        (status = 204, description = "Restored or already live; empty body"),
        (status = 400, description = "Malformed or nonpositive post_id"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Administrator access required"),
        (status = 404, description = "Physical post row not found"),
        (status = 409, description = "Post token row is absent"),
        (status = 500, description = "Database failure or outcome_unknown")
    ),
    tag = "CMS"
)]
pub async fn restore_dev_post(
    State(state): State<AppState>, Extension(admin): Extension<String>, Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .restore_post_as_admin(post_id, &admin).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

Register both with per-route `authenticate_user`; DELETE uses `delete`, restore uses `post`. They receive no `Json` extractor.

- [ ] **Step 4: Complete real-router fixtures with explicit titles and orphan restore**

Every router `INSERT dev_post` includes `title`. Add these named tests: `cms_delete_restore_are_empty_idempotent_204`, `cms_non_admin_cannot_distinguish_existing_from_missing`, `cms_restore_orphan_is_409_without_audit`, and `dev_post_auth_precedes_json_and_100000_byte_limit`. The first asserts two DELETE and two RESTORE responses are empty `204`; the second asserts both targets are `403`; the third deletes the token and asserts `409` plus zero RESTORE audits; the fourth runs the missing/null/blank/malformed/`100_001`-byte body matrix from Step 1.

- [ ] **Step 5: Run GREEN and verify truly empty responses**

```bash
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-router-$(uuidgen):" \
  cargo test router::cms -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-router-$(uuidgen):" \
  cargo test router::dev_post -- --nocapture --test-threads=1
cargo fmt --all -- --check
git diff --check
```

Expected: all status precedence, title validation, transport limit, and empty-body assertions pass.

- [ ] **Step 6: Commit routes**

```bash
git add src/router/cms/path.rs src/router/cms/handler.rs src/router/cms/mod.rs \
  src/router/dev_post/handler.rs src/router/dev_post/mod.rs
git commit -m "feat(cms): expose Dev Post delete and restore"
```

### Task 9: Prove rollback, content/relationship preservation, and conditional pin restore

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`

**Interfaces:**
- Consumes final controller transactions.
- Produces trigger-driven rollback proof and exact title/body/relationship/timestamp preservation proof.

#### Step 1 reference: Pre-commit rollback injections

For separate `#[sqlx::test]` databases, create one of these triggers before admin DELETE:

```sql
CREATE FUNCTION fail_pin_cleanup() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'injected pin cleanup failure'; END $$;
CREATE TRIGGER fail_pin_cleanup BEFORE DELETE ON dev_post_pin
FOR EACH ROW EXECUTE FUNCTION fail_pin_cleanup();

CREATE FUNCTION fail_post_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'injected post update failure'; END $$;
CREATE TRIGGER fail_post_update BEFORE UPDATE ON dev_post
FOR EACH ROW EXECUTE FUNCTION fail_post_update();

CREATE FUNCTION fail_audit_insert() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'injected audit failure'; END $$;
CREATE TRIGGER fail_audit_insert BEFORE INSERT ON dev_post_moderation_log
FOR EACH ROW EXECUTE FUNCTION fail_audit_insert();
```

Each failed DELETE must leave the exact original `(title, body, deleted_at=NULL)`, pin count `1`, and audit count `0`. A restore-audit failure must leave the post deleted, pin absent, title/body unchanged, and only the preceding successful DELETE audit.

Name the four tests `rollback_on_pin_cleanup_failure`, `rollback_on_post_update_failure`, `rollback_on_audit_insert_failure`, and `rollback_restore_when_audit_insert_fails`. Each calls the real controller method and the common assertion:

```rust
async fn assert_live_pinned_content_without_audit(pool: &sqlx::PgPool, post_id: i64) {
    let row: (String, String, bool, i64, i64) = sqlx::query_as(
        "SELECT title,body,deleted_at IS NULL, \
           (SELECT count(*) FROM dev_post_pin WHERE post_id=$1), \
           (SELECT count(*) FROM dev_post_moderation_log WHERE post_id=$1) \
         FROM dev_post WHERE id=$1",
    ).bind(post_id).fetch_one(pool).await.unwrap();
    assert_eq!(row, ("Moderation title".into(), "Moderation body".into(), true, 1, 0));
}
```

- [ ] **Step 1a: Add and run the pin-cleanup rollback test**

Add `rollback_on_pin_cleanup_failure` with its one trigger, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::rollback_on_pin_cleanup_failure \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1b: Add and run the post-update rollback test**

Add `rollback_on_post_update_failure` with its one trigger, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::rollback_on_post_update_failure \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1c: Add and run the delete-audit rollback test**

Add `rollback_on_audit_insert_failure` with its one trigger, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::rollback_on_audit_insert_failure \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1d: Add and run the restore-audit rollback test**

Add `rollback_restore_when_audit_insert_fails` with its one trigger, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::rollback_restore_when_audit_insert_fails \
  -- --nocapture --test-threads=1
```

- [ ] **Step 2: Run rollback conformance**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::rollback_ -- --nocapture
```

Expected: all stage failures return `InternalError` and every earlier mutation rolls back.

#### Step 3 reference: Exact preservation and conditional-pin test

Seed title `"  Preserve\nTitle  "`, body `"Preserve\nbody"`, one image, like, poll, two options, vote, an expired `closes_at`, fixed created/updated/edited timestamps, and a pin. Snapshot content/timestamps and relation counts, DELETE then RESTORE, and assert:

```rust
assert_eq!(after_content, before_content);
assert_eq!(after_times, before_times);
assert_eq!(after_closes_at, before_closes_at);
assert_eq!(after_relations, (1_i64, 1_i64, 1_i64, 2_i64, 1_i64));
assert_eq!(pin_count_after_real_restore, 0);
assert!(controller.get_post_base(post_id).await.unwrap().poll.unwrap().is_closed);
```

Pin the now-live post, issue a no-op RESTORE, assert `changed=false` and pin count remains `1`. Query audit/log fields and assert neither exact content string occurs.

- [ ] **Step 3a: Build and snapshot the preservation fixture**

Seed the titled post and each named relationship/timestamp, then capture the exact
before-state tuples and counts.

- [ ] **Step 3b: Assert DELETE/RESTORE preservation**

Run DELETE then RESTORE and add only the content, timestamp, relationship, poll-expiry,
and historical-pin assertions from the reference.

- [ ] **Step 3c: Assert no-op pin preservation and content-free logs**

Pin the restored live post, run the no-op RESTORE, and add only the `changed=false`, pin
count, and audit/log privacy assertions.

#### Step 4 reference: Non-snapshot relationship race test

After DELETE, concurrently invoke RESTORE, title/body edit, a new like, removal of a distinct existing like, and a vote. Assert each successful unrelated mutation remains reflected, the removed distinct like is never resurrected, and final `(title, body)` equals the edit only when edit returned success. This documents existing races without promising a frozen deletion-time snapshot.

- [ ] **Step 4a: Build the deleted-post race fixture and barriers**

Seed the post/relationships, DELETE it, and install the deterministic start/result
coordination used by the five concurrent operations.

- [ ] **Step 4b: Run the race and assert returned outcomes**

Run RESTORE/edit/like/unlike/vote together and add only the outcome-conditioned content
and relationship assertions from the reference.

- [ ] **Step 5: Run preservation/race tests**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::restore_preserves -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::restore_race -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: moderation changes no content/relationship/timestamp outside `deleted_at` and audit timestamp; race assertions follow returned outcomes.

- [ ] **Step 6: Commit conformance tests**

```bash
git add src/controllers/dev_post/moderation_tests.rs
git commit -m "test(dev-post): prove moderation rollback and preservation"
```

### Task 10: Qualify same-primary, pgactive, and late-cache-fill concurrency boundaries

**Files:**
- Modify: `src/controllers/dev_post/moderation_tests.rs`
- Modify: `src/services/dev_post/mod.rs`
- Modify: `src/services/dev_post/moderation_tests.rs`

**Interfaces:**
- Produces deterministic same-primary lock evidence, opt-in pgactive convergence qualification, bounded stale-fill evidence, and old-generation isolation.

#### Step 1 reference: Deterministic same-primary transaction tests

Use fixed audit UUIDs and `tokio::join!` for two DELETEs and two RESTOREs; sort returned `changed` booleans and assert `[false,true]`. Fetch each audit by its supplied UUID and assert action/changed matches its returned context. For concurrent DELETE/RESTORE, derive final deleted state from returned `restore.changed`, not timestamp/UUID order.

Add pin/DELETE and live-RESTORE/pin races with a two-second timeout. A deleted post must have zero pins; live no-op restore plus pin must finish without deadlock and retain one pin.

Name the tests `concurrent_same_primary_deletes_and_restores_have_one_local_change`, `concurrent_same_primary_delete_restore_final_state_matches_context`, `pin_delete_race_cannot_leave_deleted_post_pinned`, and `live_restore_pin_race_finishes_without_deadlock`. Their core winner assertion is:

```rust
let (left, right) = tokio::join!(
    controller(pool.clone()).delete_post_as_admin(post_id, ADMIN, left_id),
    controller(pool.clone()).delete_post_as_admin(post_id, ADMIN, right_id),
);
let mut changed = vec![committed(left.unwrap()).changed, committed(right.unwrap()).changed];
changed.sort_unstable();
assert_eq!(changed, vec![false, true]);
```

- [ ] **Step 1a: Add same-primary DELETE/RESTORE serialization tests**

Add the two fixed-audit-UUID serialization tests, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::concurrent_same_primary_deletes_and_restores_have_one_local_change \
  -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::concurrent_same_primary_delete_restore_final_state_matches_context \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1b: Add the pin/DELETE race test**

Add `pin_delete_race_cannot_leave_deleted_post_pinned` with its two-second timeout, then
run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::pin_delete_race_cannot_leave_deleted_post_pinned \
  -- --nocapture --test-threads=1
```

- [ ] **Step 1c: Add the live-RESTORE/pin deadlock test**

Add `live_restore_pin_race_finishes_without_deadlock` with its two-second timeout, then
run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test \
  controllers::dev_post::moderation_tests::live_restore_pin_race_finishes_without_deadlock \
  -- --nocapture --test-threads=1
```

- [ ] **Step 2: Prove admin revocation waits on the real controller lock**

Block the post row in one transaction, start controller moderation so it first acquires admin `FOR KEY SHARE`, poll `pg_stat_activity` until it waits on the post lock, then start `DELETE FROM admin`. Assert revocation cannot finish within 100ms, release the blocker, assert moderation then revocation complete, and a later moderation request is `403`.

Name it `controller_moderation_holds_admin_lock_until_commit_then_revocation_wins`; wrap the 100ms assertion in:

```rust
assert!(tokio::time::timeout(std::time::Duration::from_millis(100), &mut revoke)
    .await.is_err(), "admin revocation bypassed FOR KEY SHARE");
post_blocker.commit().await.unwrap();
committed(moderation.await.unwrap().unwrap());
revoke.await.unwrap().unwrap();
```

- [ ] **Step 3: Run same-primary tests**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::concurrent_same_primary -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::pin_delete_race -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::live_restore_pin_race_finishes_without_deadlock -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests::controller_moderation_holds_admin_lock -- --nocapture
```

Expected: local serialization and lock order pass without using random timing as the state oracle.

#### Step 4 reference: Opt-in pgactive convergence qualification

When both `PGACTIVE_WRITER_A_URL` and `PGACTIVE_WRITER_B_URL` exist, create unique valid EIP-55 admin/token/title-aware post/pin on writer A, wait up to 30s for exact visibility on B, concurrently DELETE on A and RESTORE on B with unique audit UUIDs, then wait until both writers agree on one post state and both audit UUIDs exist on both writers. Assert only valid local action/changed values and unique IDs. Issue a post-convergence DELETE retry and wait until both writers show deleted/unpinned and public feeds omit the post. If either URL is absent, print one explicit qualification-skip line and return without claiming success.

- [ ] **Step 4a: Build the pgactive fixture and prove writer-B visibility**

Create the unique canonical admin/token/titled post/pin on writer A and wait up to 30
seconds for exact visibility on writer B.

- [ ] **Step 4b: Run the cross-writer DELETE/RESTORE attempt**

Use fixed unique audit UUIDs, run DELETE on A and RESTORE on B concurrently, and retain
both local contexts without asserting a global winner.

- [ ] **Step 4c: Prove convergence and replicated audit attempts**

Wait until both writers agree on one valid post state and both exact audit UUIDs exist on
both writers; assert only valid local `action`/`changed` observations.

- [ ] **Step 4d: Prove post-convergence retry and public hiding**

Issue the idempotent DELETE retry, wait for deleted/unpinned convergence, and assert both
public feeds omit the post.

#### Step 5 reference: Exact cache-fill hook API

Add `late_feed_fill_is_ttl_bounded`, `late_detail_fill_is_ttl_bounded`, `late_trending_fill_is_ttl_bounded`, and `late_old_generation_ranking_fill_is_invisible`. Each creates its own pair through:

```rust
#[cfg(test)]
#[derive(Clone)]
struct CacheFillHook {
    after_database_read: std::sync::Arc<tokio::sync::Barrier>,
    allow_cache_set: std::sync::Arc<tokio::sync::Barrier>,
}

#[cfg(test)]
#[derive(Clone)]
struct CacheFillBarriers {
    feed: CacheFillHook,
    detail: CacheFillHook,
    trending: CacheFillHook,
    ranking: CacheFillHook,
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum CacheFamily { Feed, Detail, Trending, Ranking }

#[cfg(test)]
impl DevPostService {
    fn with_cache_fill_barriers(mut self, barriers: CacheFillBarriers) -> Self {
        self.cache_fill_barriers = Some(barriers);
        self
    }
}
```

The assertions shared by feed/detail/trending are:

```rust
assert_eq!(postgres_content(&pool, post_id).await,
    ("New title".into(), "New description".into()));
let pttl: i64 = redis_pttl(&exact_new_namespace_key).await;
assert!((1..=1_000).contains(&pttl), "late fill PTTL={pttl}");
delete_exact_key(&exact_new_namespace_key).await;
```

The ranking test records generation `N`, releases its delayed SET only after `INCR` returns `N+1`, asserts the exact `v2:N:*` key may exist, and asserts `get_ranking` at `N+1` does not return its poisoned aggregate. `generation_read_failure_bypasses_get_and_set` injects the existing generation-read failure seam and asserts a PostgreSQL result plus no exact ranking payload.

- [ ] **Step 5a: Add the feed late-fill test**

Add `late_feed_fill_is_ttl_bounded` with its dedicated barriers, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-late-feed-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::late_feed_fill_is_ttl_bounded \
  -- --nocapture --test-threads=1
```

- [ ] **Step 5b: Add the detail late-fill test**

Add `late_detail_fill_is_ttl_bounded` with its dedicated barriers, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-late-detail-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::late_detail_fill_is_ttl_bounded \
  -- --nocapture --test-threads=1
```

- [ ] **Step 5c: Add the trending late-fill test**

Add `late_trending_fill_is_ttl_bounded` with its dedicated barriers, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-late-trending-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::late_trending_fill_is_ttl_bounded \
  -- --nocapture --test-threads=1
```

- [ ] **Step 5d: Add ranking generation race/failure tests**

Add `late_old_generation_ranking_fill_is_invisible` and
`generation_read_failure_bypasses_get_and_set`, then run:

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-ranking-race-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::late_old_generation_ranking_fill_is_invisible \
  -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-generation-failure-red-$(uuidgen):" cargo test \
  services::dev_post::moderation_tests::generation_read_failure_bypasses_get_and_set \
  -- --nocapture --test-threads=1
```

- [ ] **Step 6: Run cache-hook RED**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-race-red-$(uuidgen):" \
DEVPOST_FEED_EXPIRATION=1000 DEVPOST_DETAIL_EXPIRATION=1000 \
DEVPOST_TRENDING_EXPIRATION=1000 DEVPOST_RANKING_EXPIRATION=1000 \
  cargo test services::dev_post::moderation_tests::late_ -- --nocapture --test-threads=1
```

Expected RED: compile failure because `CacheFillHook`, four-family `CacheFillBarriers`, `cache_fill_barriers`, and `with_cache_fill_barriers` are absent.

- [ ] **Step 7: Install hooks at the four exact cache-fill points**

Add `#[cfg(test)] cache_fill_barriers: Option<CacheFillBarriers>` to `DevPostService`; `new` initializes it to `None`. Add:

```rust
#[cfg(test)]
async fn pause_cache_fill(&self, family: CacheFamily, after_read: bool) {
    let Some(barriers) = &self.cache_fill_barriers else { return };
    let hook = match family {
        CacheFamily::Feed => &barriers.feed,
        CacheFamily::Detail => &barriers.detail,
        CacheFamily::Trending => &barriers.trending,
        CacheFamily::Ranking => &barriers.ranking,
    };
    if after_read { hook.after_database_read.wait().await; }
    else { hook.allow_cache_set.wait().await; }
}
```

Call `pause_cache_fill(family, true)` immediately after each controller result returns and `pause_cache_fill(family, false)` immediately before the corresponding Redis SET in exactly these four miss paths: `get_feed` → `set_devpost_feed_base`; `get_post` → `set_devpost_detail_base`; `get_trending` → `set_devpost_trending_base`; `get_ranking` → `set_devpost_ranking_response_for_generation(captured_generation, ...)`. Under `#[cfg(not(test))]` no hook field/call is compiled.

- [ ] **Step 8: Run isolated concurrency/cache GREEN**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test -n "${PGACTIVE_WRITER_A_URL:-}" || exit 1
test -n "${PGACTIVE_WRITER_B_URL:-}" || exit 1
test "$PGACTIVE_WRITER_A_URL" != "$PGACTIVE_WRITER_B_URL" || exit 1
test -z "${DATABASE_URL:-}" || test "$PGACTIVE_WRITER_A_URL" != "$DATABASE_URL" || exit 1
test -z "${DATABASE_URL:-}" || test "$PGACTIVE_WRITER_B_URL" != "$DATABASE_URL" || exit 1
test "${PGACTIVE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || {
  echo 'set PGACTIVE_TEST_CONFIRM_DISPOSABLE=1 only for disposable migrated qualification writers' >&2
  exit 1
}
DATABASE_URL="$DATABASE_TEST_URL" cargo test pgactive_cross_writer_moderation_converges_and_retry_is_safe -- --nocapture --test-threads=1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-race-green-$(uuidgen):" \
DEVPOST_FEED_EXPIRATION=1000 DEVPOST_DETAIL_EXPIRATION=1000 \
DEVPOST_TRENDING_EXPIRATION=1000 DEVPOST_RANKING_EXPIRATION=1000 \
  cargo test services::dev_post::moderation_tests::late_ -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-generation-green-$(uuidgen):" \
DEVPOST_FEED_EXPIRATION=1000 DEVPOST_DETAIL_EXPIRATION=1000 \
DEVPOST_TRENDING_EXPIRATION=1000 DEVPOST_RANKING_EXPIRATION=1000 \
  cargo test services::dev_post::moderation_tests::generation_read_failure -- --nocapture --test-threads=1
cargo fmt --all -- --check
git diff --check
```

Expected: pgactive passes only on explicitly confirmed disposable writers; all four late-fill tests run with all four TTL lazy statics set to exactly `1_000ms` in each Cargo process and pass deterministically.

- [ ] **Step 9: Commit concurrency qualification**

```bash
git add src/controllers/dev_post/moderation_tests.rs src/services/dev_post/mod.rs \
  src/services/dev_post/moderation_tests.rs
git commit -m "test(dev-post): qualify moderation and cache races"
```

### Task 11: Publish OpenAPI, API docs, rollout, and rollback contracts

**Files:**
- Modify: `src/main.rs`
- Modify: `docs/dev-post-api.md`
- Modify: `docs/V2_API_CHANGES.md`
- Create: `docs/cms-dev-post-moderation.md`

**Interfaces:**
- Produces generated OpenAPI with unchanged 13 Dev Post operations plus 2 CMS operations and exact title schema nullability/requiredness.
- Produces an executable maintenance-window runbook and post-backfill roll-forward-only boundary.

- [ ] **Step 1: Add failing generated-OpenAPI assertions**

Assert JSON components and paths directly:

```rust
#[test]
fn openapi_requires_title_and_registers_exact_moderation_contract() {
    let openapi = ApiDoc::openapi();
    let json = serde_json::to_value(openapi).unwrap();
    let schemas = &json["components"]["schemas"];
    assert!(schemas["CreateDevPostRequest"]["required"].as_array().unwrap()
        .contains(&serde_json::json!("title")));
    assert_eq!(schemas["CreateDevPostRequest"]["properties"]["title"]["type"], "string");
    assert!(schemas["CreateDevPostRequest"]["properties"]["title"].get("maxLength").is_none());
    assert!(!schemas["EditDevPostRequest"]["required"].as_array()
        .is_some_and(|fields| fields.contains(&serde_json::json!("title"))));
    assert_eq!(schemas["EditDevPostRequest"]["properties"]["title"]["type"], "string");
    assert_ne!(schemas["EditDevPostRequest"]["properties"]["title"]["nullable"], true);
    assert!(schemas["DevPostResponse"]["required"].as_array().unwrap()
        .contains(&serde_json::json!("title")));
    assert!(json["paths"]["/cms/dev-post/{post_id}"]["delete"].is_object());
    assert!(json["paths"]["/cms/dev-post/{post_id}/restore"]["post"].is_object());
    assert!(json["paths"]["/dev-post"]["post"]["responses"]["413"].is_object());
    assert!(json["paths"]["/dev-post/{post_id}"]["patch"]["responses"]["413"].is_object());
    assert!(json["paths"]["/cms/dev-post/{post_id}"]["delete"]["responses"]["204"]
        .get("content").is_none());
}
```

In the same named test, count operations tagged `DevPost` as exactly 13 and the two new paths tagged `CMS` as exactly 2. Assert create/edit document `400/413`, DELETE documents `204/400/401/403/404/500`, RESTORE documents `204/400/401/403/404/409/500`, and both `204` responses have no content schema.

- [ ] **Step 2: Run OpenAPI RED before registration and annotations**

```bash
cargo test openapi_requires_title_and_registers_exact_moderation_contract -- --nocapture
```

Expected RED: required title/schema assertions and CMS paths or `413` response entries are absent from generated OpenAPI.

- [ ] **Step 3: Register paths/schemas and run OpenAPI GREEN**

Add CMS handlers to `#[openapi(paths(...))]`; existing request/response types remain registered through their handler references. Run:

```bash
cargo test openapi_requires_title_and_registers_exact_moderation_contract -- --nocapture
```

Expected: all required/non-null/no-`maxLength`, 13+2, status, and empty-204 assertions pass.

- [ ] **Step 4: Update public API references with separate content semantics**

In both existing docs, make examples use:

```json
{"token_id":"0x...","title":"Exact title\nwith newline","body":"Description only"}
```

Document create missing/null/blank `400`; edit omitted-preserve versus null/blank `400`; exact whitespace preservation; no title-specific cap; whole-request `413`; description-only tweet parsing; all response surfaces; accepted old-reader misrender, old-create `400`, legacy body-only edit ambiguity; no negotiation/versioned endpoint/fallback; and 13 Dev Post + 2 CMS counts.

- [ ] **Step 5: Write the CMS operations/runbook document**

`docs/cms-dev-post-moderation.md` must include exact endpoints/statuses, admin-first EIP-55 lock order, idempotent audit semantics, commit reconciliation/outcome-unknown retry, content/relationship preservation, conditional pin restore, pgactive limitation, exact cache families, TTL/late-fill bound, and this rollout order:

1. Publish/apply audit schema readiness; notify all consumers of immediate break.
2. Gate all Dev Post reads/writes and CMS controls, including direct API callers; drain every pre-title node.
3. Record row count/available samples; never insert production samples; verify backups, WAL/free space, replica health, and all four TTLs `<=60_000ms`.
4. Apply `0041` title migration. Its commit is the point of no return for pre-title binaries.
5. Verify title schema, row count, representative available rows, disposable-fixture shapes, audit schema, and replica convergence.
6. Deploy title-aware API only; verify feed v3/detail v2/trending v2/ranking generation code on every node.
7. Delete exactly known prefixed global feed legacy/v2 keys, known token/detail keys from controlled inputs, and trending legacy/v2 keys; increment the exact prefixed generation key. Unknown old keys expire naturally; never enumerate.
8. Run authenticated create/edit/read/title validation/auth-precedence smoke tests; then enable the breaking Dev Post contract and CMS controls.
9. Monitor create/edit `400`/`413`, replica lag, cache PTTL/late fills, old-schema cache access, invalidation/generation warnings, lock/deadlock, audit/reconciliation/outcome-unknown, and title/body rendering.

Rollback text must say: before title migration commit, abort/rollback transaction and restore old binary/traffic; after commit, never run a pre-title binary, concatenate body, drop title, or restore a compatibility response. Keep traffic gated and roll forward with a title-aware corrective binary. CMS controls alone may be disabled safely while audit/content remain authoritative.

- [ ] **Step 6: Add documentation negative assertions**

Add a test in `src/main.rs` reading the three docs and asserting they contain `title`, `description only`, `point of no return`, `13`, and `2`, and do not contain capability-header, versioned-path, or combined-body promises. Use exact banned phrases tied to old contract examples, not the runbook's explicit statement that those mechanisms are absent.

- [ ] **Step 7: Run docs/OpenAPI/source checks**

```bash
cargo test openapi -- --nocapture
cargo test documentation_contract -- --nocapture
rg -n 'title.*\+.*body|body.*title.*description' docs/dev-post-api.md docs/V2_API_CHANGES.md \
  docs/cms-dev-post-moderation.md && exit 1 || true
cargo fmt --all -- --check
git diff --check
```

Expected: contracts pass and no stale combined-content example remains.

- [ ] **Step 8: Commit documentation/contracts**

```bash
git add src/main.rs docs/dev-post-api.md docs/V2_API_CHANGES.md docs/cms-dev-post-moderation.md
git commit -m "docs(dev-post): publish moderation and title cutover"
```

### Task 12: Run full verification, two-stage review, and open the Draft API PR

**Files:**
- Verify: every changed file from Tasks 1–11
- Modify: only files implicated by review findings

**Interfaces:**
- Consumes complete implementation and final migrations squash commit.
- Produces a reviewed Draft API PR based on `v2`; it does not merge the API PR.

- [ ] **Step 1: Verify branch/base, formatting, compilation, and patch whitespace**

```bash
test "$(git branch --show-current)" = 'feat/dev-post-moderation-title'
git fetch origin v2:v2
git merge-base --is-ancestor v2 HEAD
cargo fmt --all -- --check
cargo check --all-targets --all-features
git diff --check
```

Expected: all commands exit `0`; no unreviewed branch/base drift.

- [ ] **Step 2: Run the full test matrix in disposable Postgres/Redis environments**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" \
REDIS_URL="$REDIS_TEST_URL" \
REDIS_KEY_PREFIX="test-final-$(uuidgen | tr '[:upper:]' '[:lower:]'):" \
  cargo test --all-targets --all-features -- --test-threads=1
```

Expected: every test passes with `0 failed`; pgactive either passes with configured writer URLs or prints its explicit skip.

- [ ] **Step 3: Run Clippy**

```bash
cargo clippy --all-targets --all-features
```

Expected: exit `0`; no changed-file warning.

- [ ] **Step 4: Run source-only safety scans**

```bash
rg -n 'LOWER\(|lower\(|ILIKE' src/controllers/dev_post/moderation.rs src/services/dev_post src/router/cms && exit 1 || true
rg -n 'devpost:ranking:\{page\}|get_devpost_ranking_response\(|set_devpost_ranking_response\(' src && exit 1 || true
if git diff --unified=0 v2...HEAD -- src | rg '^\+.*(KEYS|SCAN|FLUSHALL)'; then exit 1; fi
LOG_LINE_RE='^\+.*((title|body).*((tracing::)?(trace|debug|info|warn|error)!)|((tracing::)?(trace|debug|info|warn|error)!).*(title|body))'
printf '%s\n' '+ tracing::warn!(title = %value)' '+ body = value; error!("failed")' \
  | rg -i --pcre2 "$LOG_LINE_RE" >/dev/null || exit 1
if git diff --unified=0 v2...HEAD -- src | rg -i --pcre2 "$LOG_LINE_RE"; then exit 1; fi
LOG_BLOCK_RE='(?is)(?:tracing::)?(?:trace|debug|info|warn|error)!\((?:(?!\);)[\s\S])*\b(?:title|body)\b(?:(?!\);)[\s\S])*\);'
printf 'tracing::warn!(\n  body = %%value,\n);\n' \
  | rg -U --pcre2 "$LOG_BLOCK_RE" >/dev/null || exit 1
CHANGED_SRC="$(git diff --name-only v2...HEAD -- src)"
if test -n "$CHANGED_SRC" && rg -n -U --pcre2 \
  "$LOG_BLOCK_RE" \
  $CHANGED_SRC; then exit 1; fi
```

Expected: both one-line token orders and a multiline unsafe fixture are detected by the
scan itself; changed source contains no case-folded address SQL, legacy ranking caller,
Redis enumeration, or content logging. Documentation is deliberately excluded.

- [ ] **Step 5: Verify migration remote/gitlink/symlink identity and immutable published names**

```bash
git -C migrations fetch origin v2:v2
test "$(git -C migrations rev-parse HEAD)" = "$(git -C migrations rev-parse origin/v2)"
test "$(git rev-parse HEAD:migrations)" = "$(git -C migrations rev-parse HEAD)"
test "$(readlink migrations-test/0040_dev_post_moderation_log.sql)" = '../migrations/0040_dev_post_moderation_log.sql'
test "$(readlink migrations-test/0041_dev_post_title.sql)" = '../migrations/0041_dev_post_title.sql'
git -C migrations ls-tree --name-only origin/v2 | rg '^0040_dev_post_moderation_log\.sql$|^0041_dev_post_title\.sql$'
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || {
  echo 'DATABASE_TEST_URL must be a disposable non-production PostgreSQL server' >&2
  exit 1
}
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" cargo test --test dev_post_content_migrations -- --nocapture
```

Expected: API gitlink equals final migrations `origin/v2`; both exact published migrations and symlinks pass the executable contract.

- [ ] **Step 6: Run focused suites with fresh process namespaces**

```bash
test "${DATABASE_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${DATABASE_TEST_URL:-}" || exit 1
test -z "${DATABASE_URL:-}" || test "$DATABASE_TEST_URL" != "$DATABASE_URL" || exit 1
test "${REDIS_TEST_CONFIRM_DISPOSABLE:-}" = 1 || exit 1
test -n "${REDIS_TEST_URL:-}" || exit 1
test -z "${REDIS_URL:-}" || test "$REDIS_TEST_URL" != "$REDIS_URL" || exit 1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-final-service-$(uuidgen):" \
  cargo test services::dev_post -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-final-router-$(uuidgen):" \
  cargo test router::dev_post -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" REDIS_URL="$REDIS_TEST_URL" REDIS_KEY_PREFIX="test-final-cms-$(uuidgen):" \
  cargo test router::cms -- --nocapture --test-threads=1
DATABASE_URL="$DATABASE_TEST_URL" cargo test controllers::dev_post::moderation_tests -- --nocapture --test-threads=1
```

Expected: focused cache/router/controller contracts pass without shared Redis namespace assumptions.

- [ ] **Step 7: Dispatch Opus 4.8 spec-compliance review**

Use `superpowers:requesting-code-review` with:

```text
Compare implementation to both approved designs and integrated plan. Check create/edit title
wire states and exact storage; first-LF/CRLF migration; all response/cache surfaces; immediate
break/no fallback; CMS privacy/idempotency/audit/preservation; commit reconciliation; exact
cache versions and generation policy; TTL/late-fill bound; pgactive claims; OpenAPI 13+2;
migration publication gate; rollout point of no return and rollback. Report file/line severity.
```

- [ ] **Step 8: Dispatch Fable 5 independent code-quality review**

Use `superpowers:requesting-code-review` with:

```text
Review transaction consumption/rollback, SQL lock order, EIP-55 exactness, edit tristate serde
and utoipa consistency, UUID audit matching, captured generation, exact invalidation isolation,
auth/extractor/body-limit order, deterministic tests, disposable environment guards, source-only
safety scans, migration ancestry/gitlink, and unrelated diff. Report file/line severity.
```

- [ ] **Step 9: Resolve findings through TDD and repeat the full gate**

For each Critical/Important finding: add the smallest named failing test, run its fully-qualified command, patch minimally, rerun it, commit `fix(dev-post): address integrated review findings`, then repeat Steps 1–8. A Minor may remain only if the PR handoff names file/line and rationale.

- [ ] **Step 10: Record final self-check evidence**

```bash
git status --short
git log --oneline v2..HEAD
git diff --stat v2...HEAD
git diff --check
```

Expected: clean worktree and only scoped docs/migration gitlink/Rust/tests changes.

- [ ] **Step 11: Push the reviewed API feature branch**

```bash
git push -u origin feat/dev-post-moderation-title
test "$(git rev-parse HEAD)" = "$(git ls-remote origin refs/heads/feat/dev-post-moderation-title | cut -f1)"
```

Expected: remote head is exactly the reviewed local head.

- [ ] **Step 12: Open and verify the Draft API PR**

```bash
gh pr create \
  --base v2 \
  --head feat/dev-post-moderation-title \
  --draft \
  --title "feat(dev-post): add CMS moderation and explicit titles" \
  --body "Ships required explicit title/description storage and immediate breaking cache cutover together with audited idempotent CMS delete/restore. Includes ordered published migrations, commit reconciliation, exact v3/v2 invalidation, generation-scoped ranking, title/content preservation, OpenAPI/docs, concurrency qualification, and maintenance rollout/roll-forward boundary."
gh pr view --json number,url,baseRefName,headRefName,isDraft,headRefOid
```

Expected: Draft PR base is `v2`, head is the fresh feature branch, `isDraft=true`, and `headRefOid` equals local HEAD. Report PR URL, migrations PR/final SHA, API commits, full test/check/review results, pgactive status, accepted immediate old-client break, and eventual-consistency TTL bound. Do not merge the API PR without a new user instruction.

## Final Verification Checklist

- [ ] Both stale specs and the old plan link to this integrated plan and cannot be mistaken for executable work.
- [ ] One explicitly approved migrations PR published ordered `0040` audit then `0041` title while both were unpublished; any execution-time collision preserved published filenames and moved only unpublished work.
- [ ] Final API gitlink/symlinks equal migrations `origin/v2`; disposable migration tests prove schema, first-LF/CRLF split, unchanged rows/relationships, and audit survival/no FKs.
- [ ] Create missing/null/blank title is `400`; edit omission/null/value remain distinct; accepted title bytes round-trip with no title cap; whole-request limit remains `413`.
- [ ] PostgreSQL, feed pin/posts, detail, trending, read-after-write, serde fixtures, and new cache payloads expose required title plus description-only body; tweet parsing uses body only.
- [ ] New readers use feed v3/detail v2/trending v2/ranking generation v2 only; exact invalidation covers all specified old/new families with correct create/edit/pin/delete/restore generation policy.
- [ ] CMS auth/privacy/status/empty-204 contracts, audit idempotency, orphan policy, shared author delete, rollback, commit reconciliation, title/body preservation, and conditional pin restore are proven.
- [ ] Same-primary assertions use returned contexts/exact audit UUIDs; pgactive assertions remain convergence-compatible; late fills are barrier-driven and PTTL-bounded.
- [ ] No address case folding, Redis enumeration/flush, content logging, capability negotiation, versioned endpoint, combined-body fallback, moderation reason/body, hard delete, or audit read API was introduced.
- [ ] OpenAPI/docs expose exact 13 Dev Post + 2 CMS operations and the maintenance gate, migration point of no return, roll-forward-only post-backfill recovery, and monitoring contract.
- [ ] Full fmt/check/test/clippy, source-only scans, migration identity checks, two-stage review, and Draft API PR verification are green.

## Plan Self-Review Record

- **Spec coverage:** title design Sections 1–7 map to Tasks 2–4; cache/CMS Sections 8–9 map to Tasks 5–10; docs/tests/rollout/rollback Sections 10–14 map to Tasks 8–12. Moderation Sections 1–9 map to Tasks 2 and 6–9; cache/concurrency/logging Sections 10–13 map to Tasks 5, 7, and 10; migration/docs/tests/operations Sections 14–18 map to Tasks 1–2 and 8–12.
- **Placeholder audit:** every production edit has an exact file, signature/SQL or bounded behavioral contract, command, and expected result. Conditional migration renumbering is an explicit stop/reconcile procedure, not deferred design.
- **Type consistency:** `EditTitle` flows types → controller; `CommitOutcome<T>` flows controller → service; CMS reconciliation uses the exact `ModerationContext`/`ModerationAudit`; Redis ranking methods consistently require captured generation; handlers call the exact service method names and return `AppResult<StatusCode>`.
- **Boundary audit:** moderation SQL remains in `DevPostController`; `CmsController` is untouched; ranking aggregates do not gain title; like/unlike/vote invalidation is not broadened; the immediate client break does not add compatibility machinery.
- **Operational audit:** migrations publication has a hard user-approval pause; every database/Redis qualification fails closed without explicit disposable URLs; post-backfill rollback is roll-forward only; API publication ends at a Draft PR.
