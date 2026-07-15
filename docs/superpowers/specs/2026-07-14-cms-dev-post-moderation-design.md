# CMS Dev Post Moderation — Design Spec

> [!IMPORTANT]
> **APPROVED DECISION RECORD — EXECUTE ONLY THE INTEGRATED PLAN.** The moderation
> decisions in this document remain authoritative, but implementation is unified with
> explicit title/body separation by
> [`../plans/2026-07-14-cms-dev-post-moderation-and-title.md`](../plans/2026-07-14-cms-dev-post-moderation-and-title.md).
> Do not execute the historical moderation-only plan.

- Date: 2026-07-14
- Status: Approved decision record; superseded for execution by the integrated plan
- Integration base: `v2` (`2c778a9` at design time)
- Scope: Rust API, PostgreSQL migration, Redis invalidation, OpenAPI, operational docs, tests

## 1. Summary

CMS administrators need a minimal way to remove a Dev Post by its `post_id` and restore
it later. Removal remains a soft delete; moderation preserves both the explicit `title`
and description-only `body` byte-for-byte and does not delete relationship rows. Every
successful CMS delete or restore request is recorded in a permanent audit table in the
same transaction as the state change.

The public author-delete path and the CMS path share one Dev Post deletion core.
Authorization remains separate: an author can delete only their own live post, while an
administrator can idempotently delete or restore any existing post. Within one database
transaction, both deletion paths remove the locally observed active pin atomically and
use the same full cache invalidation policy after a confirmed or ambiguous commit
attempt.

New CMS endpoints:

```http
DELETE /cms/dev-post/{post_id}
POST   /cms/dev-post/{post_id}/restore
```

Both return an empty `204 No Content` on success, including valid no-op repetitions.
They ship in the same API PR as the immediate breaking title/body separation and use the
same feed v3, detail v2, trending v2, and generation-scoped ranking cache contract.

## 2. Problem and goals

The CMS page has no convenient post lookup workflow yet. The first version therefore accepts a manually entered `post_id`. It must:

- let an authenticated administrator soft-delete an existing Dev Post;
- let an authenticated administrator restore a soft-deleted Dev Post;
- make both CMS operations idempotent for an existing post;
- auto-unpin a deleted post in the same database transaction;
- restore a post as an ordinary post, without resurrecting a historical pin;
- avoid intentionally mutating images, likes, poll, poll options, votes, author, `title`,
  description-only `body`, and original timestamps as part of moderation;
- permanently audit every successful CMS request, including no-op repetitions;
- avoid revealing whether a valid positive post ID exists to a non-admin;
- keep authorization and all mutations on the primary database;
- invalidate all affected legacy and feed v2/v3, detail v1/v2, and trending v1/v2 cache
  families after a successful author or CMS deletion, and after a CMS restore;
- share the title cutover's bounded late-cache-fill/read-replica race and captured ranking
  generation policy;
- narrowly expand create/edit invalidation as required by the integrated title/body
  contract without changing unrelated like/unlike/vote behavior;
- avoid Redis `KEYS`, `SCAN`, prefix-wide deletion, and `FLUSHALL`.

## 3. Non-goals

- A CMS post list, search UI, or lookup endpoint
- Moderation-log read/list endpoints
- A delete/restore request body or moderation reason
- Hard deletion or deletion of related images, likes, polls, options, or votes
- Restoring a post's former pin
- Changing creator/author pin authorization
- Admin pinning or editing
- Editing `title` or `body` through the CMS moderation endpoints
- Bulk delete/restore
- Scheduled deletion, retention policy, or undo window
- Strict read-after-write consistency across Redis and the PostgreSQL read replica
- Snapshotting or freezing related rows while a post is deleted; existing edit, like,
  unlike, and vote races keep their current behavior
- A general rewrite of like, unlike, vote, pin, or unpin behavior beyond the exact
  title-aware cache invalidation specified by the integrated plan

## 4. Alternatives considered

### 4.1 Shared Dev Post mutation core — selected

```text
author delete ─┐
               ├─> shared soft-delete core ─> shared full cache invalidation
CMS delete ────┘

CMS restore ─────> shared locked-post context + restore mutation
```

The entry points retain different authorization and idempotency policies, but pin cleanup, `deleted_at` mutation, transaction boundaries, and cache invalidation are centralized. This prevents the author and CMS paths from drifting.

### 4.2 Independent SQL in `CmsController` — rejected

This is initially shorter, but duplicates lock ordering, pin cleanup, soft-delete semantics, and cache invalidation. A later Dev Post change could easily update only one path.

### 4.3 Invoke author deletion by impersonating the author — rejected

Administrator authority is not author authority. Impersonation would mix audit identity, create misleading authorization semantics, retain the author's non-idempotent `404` behavior, and make race handling harder to reason about.

## 5. HTTP API

Axum 0.7 runtime paths and OpenAPI paths use different parameter syntax and must be represented separately.

| Operation | Runtime router path | OpenAPI/docs path |
|---|---|---|
| Delete | `/cms/dev-post/:post_id` | `/cms/dev-post/{post_id}` |
| Restore | `/cms/dev-post/:post_id/restore` | `/cms/dev-post/{post_id}/restore` |

Both routes use the existing `authenticate_user` middleware. They accept no body and return `AppResult<StatusCode>` with `StatusCode::NO_CONTENT`, so a successful response has no JSON and no bytes after the headers.

### 5.1 Delete

```http
DELETE /cms/dev-post/{post_id}
```

- Live existing post: set `deleted_at`, remove its pin mapping, audit with `changed=true`, return `204`.
- Already-deleted existing post: leave `deleted_at` unchanged, defensively remove any impossible stale pin mapping, audit with `changed=false`, return `204`.
- The audit `changed` field describes the `dev_post.deleted_at` transition only; defensive pin cleanup does not turn a no-op post transition into `changed=true`.

### 5.2 Restore

```http
POST /cms/dev-post/{post_id}/restore
```

- Deleted existing post with an existing token: set only `deleted_at = NULL`, ensure the old pin is absent, audit with `changed=true`, return `204`.
- Already-live existing post with an existing token: do not update it and do not touch a current pin, audit with `changed=false`, return `204`.
- A restored post returns to its ordinary `id DESC` position. The moderation statements
  themselves do not update `created_at`, `updated_at`, `edited_at`, poll close time, or
  relationship rows. This is not a snapshot guarantee: unrelated edit, like, unlike,
  and vote operations may still change their own rows under their existing concurrency
  behavior while the post is deleted or being moderated.

The conditional pin rule resolves two requirements without conflict: an actual deleted-to-live restore cannot resurrect a historical mapping, while a no-op restore of a currently live and newly pinned post preserves that current pin.

### 5.3 Status and precedence

| Situation | Status | Audit row |
|---|---:|---|
| First or repeated successful DELETE | `204` | Yes |
| First or repeated successful RESTORE | `204` | Yes |
| Missing/invalid session, including with a malformed `post_id` | `401` | No |
| Authenticated request with malformed, zero, or negative `post_id` | `400` | No |
| Authenticated non-admin with a valid positive ID | `403` | No |
| No physical `dev_post` row | `404` | No |
| RESTORE whose post's token row is absent | `409` | No |
| Database failure before `COMMIT` is attempted | `500` | No; explicitly roll back best-effort |
| `COMMIT` error, audit reconciliation confirms the row | `204` | Yes; treat as committed |
| `COMMIT` error, audit reconciliation cannot confirm the row | `500` (`outcome_unknown`) | May exist |

Authentication middleware runs before handler extraction. Therefore an unauthenticated
request receives `401` even when the dynamic path segment is malformed. After
authentication succeeds, Axum rejects a non-integer path parameter as `400`, and the
handler explicitly rejects `post_id <= 0` as `400`. For a valid positive ID,
administrator authorization is checked before any post lookup, so both an existing and
missing ID return the same `403` to a non-admin. Administrators may distinguish `404`
from `409` because this is required for remediation.

## 6. Authorization and transaction design

All authorization, locks, state changes, and the audit insert run in one transaction
from `get_write_pool()`. Read-replica data is never used to authorize or moderate a
post. The application generates the audit UUID before beginning the transaction so it
can reconcile an ambiguous `COMMIT` result against the write pool.

### 6.1 EIP-55 address rule

The session address is the EIP-55 canonical address produced by authentication. `admin.account_id`, `token_id`, and the copied audit values are compared or stored exactly as canonical strings. SQL must use exact equality and must not use `LOWER()` or case-folded comparisons. A case-mutated address does not authorize.

### 6.2 Admin-first authorization

The first database read after beginning the transaction is:

```sql
SELECT account_id
FROM admin
WHERE account_id = $1
FOR KEY SHARE;
```

No row means `403` immediately, before reading `dev_post`. On the same primary,
`FOR KEY SHARE` holds the administrator membership row through commit and blocks a
concurrent revocation by row deletion or key-changing update. Thus that transaction is
authorized against its primary state without a check/use gap. A same-primary revocation
that arrives after this lock waits for the in-flight moderation transaction; later
requests on the converged state observe the revocation and fail. This row lock is not a
cross-writer serialization claim; Section 12 defines the pgactive boundary.

### 6.3 Lock order

On one PostgreSQL primary, moderation and existing pin/delete paths use the compatible
lock order:

```text
CMS:    admin row -> optional/required token row -> post row
author:              required token row          -> post row
pin:                 required token row          -> post row
```

After the admin lock, a non-locking lookup reads immutable `dev_post.token_id` by `post_id`; absence yields `404`. It exists only to identify the token before the post lock and is safe because authorization has already succeeded.

Then:

1. Lock the matching token row with `FOR KEY SHARE` if it exists.
   - DELETE treats it as optional so an orphan post remains removable.
   - RESTORE requires it; absence returns `409` before mutation.
2. Lock the post with `FOR UPDATE`, constrained by both `id` and the previously read `token_id` and including live or deleted rows.
3. If the post disappeared before the lock, return `404`. A token ID change is unsupported and the defensive compound predicate prevents acting on a mismatched row.
4. Apply the action, insert the audit row, and commit.

Taking the token lock before the post lock matches the existing pin path and avoids a CMS delete/pin lock inversion. The token lock also prevents physical token deletion during a normal operation. The admin row is independent and always comes first only for CMS calls.

## 7. Ownership, shared controller structure, and policies

Ownership is explicit:

- The CMS router handler owns HTTP extraction, the authenticated session input, status
  mapping, and the empty `204` response. It constructs/calls `DevPostService`; it does
  not issue moderation SQL.
- `DevPostService` owns orchestration around the transaction result: calling the
  `DevPostController`, reconciling a `COMMIT` error by audit ID, invoking full
  best-effort cache invalidation, and emitting structured operational logs.
- `DevPostController` owns the primary-database transaction, admin/post/token reads and
  locks, the shared author/admin soft-delete primitives, restore mutation, audit insert,
  and the write-pool audit lookup used for commit reconciliation.
- `CmsController` owns none of this feature's SQL. Adding independent moderation SQL to
  `CmsController` would reintroduce the rejected duplicate implementation.

The controller should expose distinct policy entry points over shared internal primitives, conceptually:

```text
delete_post_as_author(post_id, author)
  -> lock required token + live post
  -> require exact post.author == author
  -> apply_soft_delete(...)

delete_post_as_admin(post_id, admin)
  -> lock admin + optional token + existing post
  -> apply_soft_delete(...)
  -> insert DELETE audit

restore_post_as_admin(post_id, admin)
  -> lock admin + required token + existing post
  -> apply_restore(...)
  -> insert RESTORE audit
```

Internal result data is prepared before commit and carries at least `post_id`,
`token_id`, `changed`, and, for CMS actions, the pre-generated `audit_id`. The service
consumes this context for commit reconciliation, invalidation, and structured logging;
it is not returned to the client.

### 7.1 Author policy remains unchanged

- Only a live post can enter the author delete core.
- Exact `dev_post.author == session_address` is required.
- Wrong author remains `403`.
- Missing or already-deleted post remains `404`.
- Its existing response contract remains unchanged.
- It does not write the CMS moderation audit table.

Only its internals and post-commit cache invalidation expand.

### 7.2 CMS policy

- Current admin membership, not author or token creator status, grants access.
- The post may be live or already deleted.
- Creator changes have no effect on CMS authorization.
- DELETE can moderate an orphan post whose `token` row is absent.
- RESTORE requires the token row because public hydration and token-scoped feeds require it; an orphan restore returns `409`.

### 7.3 Pre-commit failures and ambiguous commit outcomes

Database errors are split at the `COMMIT` boundary; they must not all be described as
ordinary rollbacks.

Before opening the transaction, the service/controller boundary prepares a new
`audit_id = Uuid::new_v4()` for the CMS attempt. The audit insert uses that exact ID,
and the pending result context retains the expected `audit_id`, `admin_account_id`,
`post_id`, `token_id`, `action`, and `changed` values.

If authorization, locking, mutation, or audit insertion fails before `COMMIT` is
attempted, explicitly call rollback best-effort and return the mapped error. No cache
invalidation or success log is emitted. Because no commit was attempted, the state
transition and audit row are both uncommitted.

If `COMMIT` succeeds, run full best-effort invalidation, emit the committed moderation
event, and return `204`.

If the `COMMIT` call itself returns an error, its outcome is unknown: PostgreSQL may
have committed even though the acknowledgement was lost. Do not claim that the
transaction rolled back and do not attempt to reuse that consumed transaction.
Instead:

1. Query `dev_post_moderation_log` by the pre-generated `audit_id` using
   `get_write_pool()` (never the read replica), best-effort.
2. Treat the action as confirmed committed only when the immutable audit row exists and
   matches the pending action context. Then run full best-effort invalidation, emit a
   reconciled-commit info event, and return `204`.
3. If the lookup returns no row, returns mismatched data, or itself fails, the outcome
   remains unconfirmed. Still run full best-effort invalidation because the mutation may
   have committed, emit a structured `outcome_unknown` error with the audit ID, and
   return `500` classified as `outcome_unknown`.

An immediate missing row is not proof of rollback in a pgactive deployment or during a
write-node connectivity incident. A client may safely retry the idempotent requested
state. The retry uses a new audit UUID: if the first attempt committed, the retry is a
`changed=false` audit; if it did not, the retry performs the transition. The audit log
therefore records accepted attempts, not exactly-once user intent.

Audit-ID reconciliation is specific to CMS attempts. Author DELETE deliberately writes
no moderation audit, so an author-delete `COMMIT` error cannot use this proof. It keeps
the existing `500` status contract with an `outcome_unknown` classification,
emits an operational outcome-unknown error, and runs the same best-effort invalidation
from its pending post/token context in case the delete committed.

## 8. Delete, unpin, restore, and relationship behavior

Moderation is content-blind. It never updates `dev_post.title` or `dev_post.body`, so
both the explicit title and description-only body survive delete, restore, repeated
no-op requests, and moderation rollback byte-for-byte.

Soft delete and unpin are atomic:

```sql
DELETE FROM dev_post_pin WHERE post_id = $1;

UPDATE dev_post
SET deleted_at = clock_timestamp()
WHERE id = $1
  AND deleted_at IS NULL;
```

The update row count determines `changed`. If either mutation or the audit insert fails
before commit is attempted, explicit best-effort rollback keeps the pin removal, post
transition, and audit insert from committing independently. A `COMMIT` error follows
the reconciliation procedure in Section 7.3 instead of being called a rollback.

Restore changes only the deletion marker. For a post that was deleted when locked, it defensively deletes any stale `dev_post_pin` row and then runs:

```sql
UPDATE dev_post
SET deleted_at = NULL
WHERE id = $1
  AND deleted_at IS NOT NULL;
```

For a post that was already live when locked, restore performs neither update nor pin deletion. This preserves a legitimate current pin and records `changed=false`.

Moderation SQL never updates `dev_post.title` or `dev_post.body` and never removes or
updates `dev_post_image`, `dev_post_like`, `dev_post_poll`, `dev_post_poll_option`, or
`dev_post_poll_vote`. Restore therefore exposes whatever related rows exist at read
time. It does not freeze a deletion-time snapshot: existing concurrent
edit/like/unlike/vote behavior remains in force, so those operations may legitimately
change their own data. Poll `closes_at` is not extended, so a poll that expired while
deleted remains closed. Physical deletion retains the existing FK cascade behavior,
but hard deletion is outside this feature.

## 9. Permanent moderation audit

Candidate migration SQL:

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

- The application generates `Uuid::new_v4()` and binds it as `id`; the schema needs no UUID extension or database default.
- There are deliberately no foreign keys to `dev_post`, `token`, `account`, or `admin`. Audit rows must survive later physical deletion or administrator removal.
- `token_id` is copied from the locked post, including for an orphan.
- Every CMS DELETE/RESTORE that is confirmed and returned as `204` inserts one row for
  that request attempt in the same transaction, even when `changed=false`.
- `changed` means the post crossed live/deleted state, not merely that defensive pin cleanup affected a row.
- `changed` is the transaction's local observation. It is not a globally unique winner
  marker across pgactive writers.
- Authorization failure, invalid input, missing post, orphan restore conflict, and
  failures before `COMMIT` do not write audit rows. An unconfirmed `COMMIT` error may
  have written one even though the response is `500 outcome_unknown`.
- Each retry is a separate audited request attempt with a new UUID; the schema does not
  promise exactly-once intent deduplication.
- There is no audit read API in this scope.
- There is no deletion reason or request body in this scope.

`clock_timestamp()` is used so the entry reflects when the moderation action executed rather than the transaction start time.

## 10. Cache invalidation

PostgreSQL is authoritative. After a successful database commit, or after a commit
error that may have committed as described in Section 7.3, one shared best-effort
helper invalidates all affected public representations for:

- author DELETE;
- CMS DELETE, including `changed=false` repetitions;
- CMS RESTORE, including `changed=false` repetitions.

Repeating invalidation on successful CMS no-ops is intentional: it can repair an earlier Redis deletion/bump failure.

The helper independently attempts:

1. global feed legacy, v2, and v3 keys;
2. token feed legacy, v2, and v3 keys for the post's `token_id`;
3. post detail legacy and v2 keys;
4. trending legacy and v2 keys;
5. ranking generation increment.

Exact old and new keys, with `REDIS_KEY_PREFIX` prepended by the shared helper, are:

```text
devpost:feed:global
devpost:feed:v2:global
devpost:feed:v3:global
devpost:feed:{token_id}
devpost:feed:v2:{token_id}
devpost:feed:v3:{token_id}
devpost:detail:{post_id}
devpost:detail:v2:{post_id}
devpost:trending
devpost:trending:v2
```

One Redis failure must not prevent later invalidation attempts and must not turn a
confirmed committed DB action into an HTTP failure. Each failure is logged as a
structured warning. An unconfirmed commit still returns `500 outcome_unknown`, but its
invalidation failures do not replace that primary error classification.

Pin PUT/DELETE keeps token-feed-only invalidation but deletes legacy, v2, and v3 token
feed keys. The integrated title work also applies exact old/new invalidation to create
and edit; it does not broaden unrelated like/unlike/vote mutation policies.

Exact deletion remains best-effort rather than strong read-after-write consistency. A
feed/detail/trending miss can read a stale replica value, pause across the authoritative
PostgreSQL commit and Redis deletion, and late-fill the new namespace afterward. The
accepted stale bound is the affected key's TTL measured from that late fill, not from
commit; feed, detail, trending, and ranking TTL configuration must each be at most
`60_000ms`. Direct uncached replica reads remain bounded by monitored replica lag.
Deterministic tests force the ordering with barriers or injected seams and assert a
positive bounded `PTTL`; they do not rely on timing luck or sleep for a production TTL.

## 11. Ranking cache generation

Ranking is cached by arbitrary `page`/`limit`, so deleting every variant by enumeration is unsafe. Introduce versioned, generation-scoped keys:

```text
generation counter: devpost:ranking:generation
ranking payload:    devpost:ranking:v2:{generation}:{page}:{limit}
legacy payload:     devpost:ranking:{page}:{limit}
```

All keys still receive `REDIS_KEY_PREFIX` through `with_prefix`.

### 11.1 Read/write algorithm

1. Read `devpost:ranking:generation` once.
2. If the key is absent, use generation `0` without writing an initializer.
3. If the generation read fails for any Redis reason, bypass ranking cache for that request: query PostgreSQL and do not read or write either legacy or v2 payload keys.
4. Use the captured generation for both cache GET and, after a miss, cache SET.
5. Never re-read generation before SET.

Capturing once prevents a query begun under generation `N` from contaminating generation `N+1`. If moderation commits and increments while that query runs, its eventual SET remains under `N` and future readers use `N+1`.

### 11.2 Invalidation

After a confirmed commit—or best-effort after an unconfirmed `COMMIT` error—invalidation
performs Redis `INCR` on the generation counter. `INCR` naturally changes a missing key
from absent/generation `0` to `1`. It is atomic across API nodes, so simultaneous
moderation invalidations produce distinct later generations without lost increments.

If the increment fails, it does not replace the database outcome: a confirmed action
still returns `204`, while an unconfirmed commit still returns `500 outcome_unknown`.
Cached ranking data may remain visible until `DEVPOST_RANKING_EXPIRATION`. This is part
of the explicit eventual-consistency contract.

### 11.3 Legacy behavior and coordinated title cutover

New code never reads or writes `devpost:ranking:{page}:{limit}`. Existing legacy ranking keys expire by TTL; no `SCAN`, `KEYS`, or wildcard delete is introduced.

Pre-title nodes still use old payload semantics and legacy ranking keys and do not
understand the generation counter. All Dev Post reads/writes and CMS controls therefore
remain gated until every pre-title node drains, the title migration commits, and only
title-aware nodes are running. Section 17 gives the exact feed/detail/trending deletion
and ranking-increment order. All commands use the deployed `REDIS_KEY_PREFIX`; unknown
keys expire naturally and operators never enumerate Redis.

Once the title backfill commits, a pre-title binary cannot safely serve reads. Recovery
is roll-forward with a title-aware binary; the legacy-ranking TTL observation is not
permission to roll back across that point of no return.

## 12. Consistency and concurrency

- Database mutation and audit insertion are atomic.
- Cache invalidation is best-effort and starts only after `COMMIT` succeeds or the
  `COMMIT` call returns an ambiguous error; it never runs for an ordinary pre-commit
  rollback.
- Public reads continue to use the existing read pool and cache-aside paths, so read-replica lag or a Redis invalidation failure may temporarily show pre-moderation state for up to the relevant TTL.
- There is no attempt to roll back a committed database action because Redis failed.
- A `COMMIT` error uses audit-ID reconciliation and may return either reconciled `204`
  or `500 outcome_unknown`; it is never automatically classified as rollback.

On the same PostgreSQL primary:

- Two concurrent CMS DELETEs serialize on the post row: both return `204`, with one
  local audit `changed=true` and the later one `changed=false`.
- Two concurrent RESTOREs behave analogously.
- Concurrent DELETE and RESTORE serialize; the operation that acquires the post lock
  second determines that primary's final state. Both successful attempts receive their
  own audit row in lock/execution order.
- Pin and delete use token-before-post locks. A delete that wins removes the pin before
  commit; a pin request that observes the deleted post fails under the existing
  live-post rule.
- A live no-op RESTORE does not remove a pin acquired before its post lock.
- Concurrent admin revocation waits behind `FOR KEY SHARE`; authorization has a clear
  transaction boundary on that primary.

Across different pgactive writers, row locks do not provide global serialization before
replication. Consequently:

- two writers may both record `changed=true` from their valid transaction-local view;
- audit timestamps and `changed` values do not define one global real-time order;
- concurrent DELETE/RESTORE or pin/DELETE final state follows pgactive replication and
  conflict-resolution/apply behavior, not a last-HTTP-request-wins promise from this API;
- the final converged post has one database state, but this design does not promise
  which concurrent writer wins; public queries continue to ignore deleted posts even
  if a cross-writer race temporarily leaves a stale pin mapping;
- after convergence, an idempotent retry drives the post toward the requested state and
  creates a new audit attempt. DELETE also defensively cleans a stale pin mapping.

Unit/integration concurrency claims that depend on row locks are therefore scoped to a
single primary. pgactive integration tests validate convergence-compatible invariants
and retry/audit behavior, not exactly one `changed=true` or a globally ordered winner.

## 13. Structured operational logs

The database audit is the permanent record. After a successful commit, emit one structured info event with:

```text
event = "cms.dev_post.moderation"
audit_id
admin_account_id
post_id
token_id
action = "DELETE" | "RESTORE"
changed
```

When a failed `COMMIT` is confirmed by audit reconciliation, emit the same event with a
`commit_reconciled=true` field. When reconciliation cannot confirm the result, emit an
error event with `event = "cms.dev_post.moderation.outcome_unknown"`, the expected
audit ID and action context, then return `500`; do not emit a false success event.

Do not log post body, images, poll content, cookies, session ID, authorization headers,
request body, or IP address. Cache invalidation failures use structured warning events
with the cache family and identifiers, without replacing the primary `204` or
`500 outcome_unknown` result.

## 14. Migration workflow

`migrations/` is a separate repository submodule. At design time the latest merged
migration is `0039_dev_post_pin.sql`, so the ordered candidates are:

```text
0040_dev_post_moderation_log.sql
0041_dev_post_title.sql
```

Recheck canonical numbering against migrations `origin/v2` immediately before creating
SQL and again before publishing. When neither candidate exists at execution time, create,
qualify, review, explicitly approve, and squash-merge both ordered files together in one
migrations branch and PR. The audit migration remains first and performs no backfill; the
title migration follows it and performs the one-time first-LF/CRLF-aware content split.

Required workflow:

1. Start a new migrations feature branch from the latest migrations `v2`.
2. Add both production migrations and qualify their ordered chain only against an
   explicitly disposable PostgreSQL URL.
3. Review both files, push the exact reviewed head, and open one Draft migrations PR
   with base `v2`.
4. Pause for explicit user approval of the reported PR number and exact reviewed SHA.
5. Recheck numbering and identity, mark ready, and squash-merge without
   `--delete-branch` only after that approval.
6. Run `git fetch origin v2:v2` in the migrations repository and verify local `v2`
   equals `origin/v2`.
7. In the fresh API feature branch based on the latest API `v2`, update the submodule
   gitlink to the final migrations squash commit and add separate numbered
   `migrations-test/` symlinks for both files.
8. Commit the gitlink, symlinks, and disposable migration contract in the API PR, whose
   base is `v2`.

The publication fallback is exact: any filename already in `origin/v2` is published and immutable; only unpublished remaining work moves to the next canonical number. A
collision requires a documentation-only reconciliation before SQL work resumes; no
published file is renamed, amended, reused, or reordered. The audit table is additive
and has no backfill, while the later title migration's content split establishes the
rollout point of no return described in Sections 17–18.

## 15. OpenAPI and documentation

Implementation must:

- add both handlers to the CMS router and `CmsPath`, with separate runtime and docs strings;
- register both handlers in the `utoipa::OpenApi` path list in `src/main.rs`;
- document required create `title`, optional create description-only `body`, the edit
  title omitted/null/value states, exact title preservation, and the whole-request
  `413` without a title-specific cap;
- expose required `title` plus description-only `body` on feed pins/posts, detail, and
  trending responses;
- document `post_id` as a positive BIGINT serialized in the URL;
- document empty `204` responses and `400/401/403/404/409/500` outcomes;
- add no request schema because neither endpoint has a body;
- update `docs/dev-post-api.md` with author/CMS delete differences, restore behavior, audit, cache consistency, and rollout;
- update `docs/V2_API_CHANGES.md` without reclassifying the CMS routes as Dev Post
  routes: the existing Dev Post count remains **13**, and this feature adds **2 CMS**
  endpoints;
- add a focused CMS moderation document if the implementation would otherwise make the general Dev Post reference unwieldy;
- correct the existing Dev Post documentation that says trending/ranking are uncached, because the current `v2` implementation does cache both.
- document feed v3/detail v2/trending v2, generation-scoped ranking, the `60_000ms`
  late-fill bound, immediate old-client break, maintenance gate, title-migration point
  of no return, and roll-forward-only post-backfill recovery.

Swagger UI (`/dev-sw`), OpenAPI JSON (`/dev-sw/openapi.json`), and Scalar (`/dev-scalar`) must expose the two routes.

## 16. Test plan

### 16.1 Migration and audit schema

- The ordered audit and title migrations apply on production-compatible and
  `migrations-test` paths from the exact final migrations squash commit.
- UUID IDs are unique and application-generated.
- A CMS attempt generates its UUID before beginning the transaction, binds the same ID
  into the audit insert, and retains it for write-pool commit reconciliation.
- `post_id > 0` and action checks reject invalid rows.
- No FK exists to post/token/admin; audit rows survive those rows being removed.
- Post/admin indexes exist.
- No backfill rows are created.
- Title is `TEXT NOT NULL DEFAULT ''` with no title index or database nonblank check.
- Legacy content splits once at the first LF, consumes an immediately preceding CR as
  part of a CRLF delimiter, preserves bare CR and later line breaks, and leaves row IDs
  and relationship rows unchanged.

### 16.2 Authorization and existence privacy

- Admin exact EIP-55 address succeeds.
- Case-mutated admin address fails; no `LOWER()` query is introduced.
- Missing session returns `401`.
- Missing/invalid session with a malformed path segment still returns `401` because
  authentication middleware runs before handler extraction.
- Non-admin receives `403` for both existing and missing valid positive IDs.
- Admin row is locked on the primary before any post access.
- Concurrent admin revocation waits, then later requests fail.
- Authenticated malformed, zero, and negative IDs return `400`.

### 16.3 CMS delete

- Live post becomes deleted, returns empty `204`, and writes `DELETE/changed=true` audit.
- Repeated delete returns empty `204` and writes a second `DELETE/changed=false` audit.
- Missing physical post returns `404` with no audit.
- Pinned post is unpinned in the same commit.
- Injected pin-cleanup, post-update, or audit-insert failures before `COMMIT` are rolled
  back and leave post/pin/audit unchanged.
- A simulated `COMMIT` error followed by an exact audit-ID match on the write pool is
  treated as committed, invalidates caches, logs reconciliation, and returns `204`.
- A simulated `COMMIT` error followed by a missing row, mismatched row, or reconciliation
  query failure performs best-effort invalidation and returns `500 outcome_unknown`;
  the test does not assert that the transaction rolled back.
- Retrying both possible `outcome_unknown` cases is safe: a previously committed action
  produces a new `changed=false` audit, while an uncommitted action performs the change
  and records `changed=true`.
- Orphan live and orphan already-deleted posts can be deleted/re-deleted and audited.
- Existing `title` and description-only `body` remain byte-identical through delete,
  repeated delete, and every pre-commit rollback injection.

### 16.4 CMS restore

- Deleted post becomes live, returns empty `204`, and writes `RESTORE/changed=true` audit.
- Live repeated restore returns empty `204` and writes `RESTORE/changed=false` audit.
- Missing physical post returns `404` with no audit.
- Orphan restore returns `409` with no audit and no state change, including an already-live orphan.
- Actual restore leaves no pin mapping and returns as an ordinary feed post.
- No-op restore of a currently pinned live post preserves the pin.
- Without concurrent relationship mutations, moderation SQL leaves images, likes,
  poll, options, and votes untouched and they are visible after restore.
- Moderation SQL does not update poll close time or original post timestamps.
- A focused race test documents that existing edit/like/unlike/vote operations retain
  their current behavior; restore is not expected to reproduce a frozen deletion-time
  snapshot.
- Audit insert failure rolls back restore.
- Existing `title` and description-only `body` remain byte-identical through restore,
  repeated restore, and every pre-commit rollback injection.

### 16.5 Author behavior and shared core

- Author can still delete their own live post with the existing response.
- Wrong author is `403`; missing/already-deleted remains `404`.
- Author delete auto-unpins atomically.
- Author delete writes no CMS moderation audit.
- Author delete preserves `title` and description-only `body` while hiding the row.
- Shared invalidation is called after successful author deletion.
- An author-delete `COMMIT` error keeps the existing `500` status contract with an
  `outcome_unknown` classification, emits the operational event, and still attempts
  full invalidation without creating or querying a CMS audit row.

### 16.6 Concurrency

- On the same primary, concurrent DELETEs yield exactly one `changed=true` and the rest
  `false`; concurrent RESTOREs behave analogously.
- On the same primary, DELETE/RESTORE final state matches lock order and every confirmed
  success is audited.
- On the same primary, pin/DELETE cannot leave a deleted post actively pinned,
  RESTORE/pin follows token-before-post order without deadlock, and admin
  revocation/moderation has the documented lock semantics.
- pgactive cross-writer tests do not assert exactly one `changed=true`, global audit
  order, or last-request-wins. They assert unique per-attempt audit IDs, valid local
  `changed` observations, convergence to a valid single post state under the deployed
  conflict policy, and safe idempotent retry after convergence.
- A pgactive pin/DELETE race may temporarily retain a stale mapping, but public reads
  hide deleted posts and a post-convergence DELETE retry removes the stale mapping.

### 16.7 Cache behavior

- Successful author delete, CMS delete, and CMS restore delete global/token feed
  legacy/v2/v3, detail legacy/v2, and trending legacy/v2 keys.
- CMS no-op repetitions retry all invalidations.
- Failure of one invalidation does not skip the remaining families or fail the request.
- Ranking generation missing means `0`; increment changes it to `1`.
- New ranking cache keys contain v2, captured generation, page, and limit.
- New readers ignore a poisoned legacy ranking key.
- Generation read error bypasses cache and does not set a payload.
- A query captured under `N` cannot write into `N+1` during a concurrent bump.
- Concurrent bumps do not lose increments.
- Ranking bump failure leaves a confirmed HTTP action successful and stale data bounded
  by TTL; it does not convert an existing `outcome_unknown` into another result.
- Tests use unique Redis keys and never `FLUSHALL`, `KEYS`, or shared-prefix scans.
- New readers and writers use only feed v3, detail v2, and trending v2 payload keys;
  old payloads are misses and no fallback reconstructs a title from cached body.
- Deterministic late-fill tests force stale replica read, commit/invalidation, and stale
  SET ordering; the resulting `PTTL` is positive and within the configured test TTL.
- Feed, detail, trending, and ranking TTL validation accepts `60_000ms` and rejects a
  larger value before the feature can be enabled.

### 16.8 Router, OpenAPI, and docs

- Real Axum requests match both `/:post_id` runtime routes.
- OpenAPI exposes both `/{post_id}` paths with the declared statuses.
- Success responses are truly empty `204` bodies.
- Authentication middleware protects both routes.
- Swagger path-registration and endpoint-count tests assert 13 Dev Post endpoints and
  2 newly added CMS moderation endpoints.
- Generated OpenAPI asserts required/non-null create title, edit omission versus invalid
  null, required response title, no title `maxLength`, and whole-request `413`.
- Dev Post and V2 change documents match implemented behavior.

### 16.9 Integrated title/body behavior

- Create missing/null/blank title is `400`; accepted title bytes are stored unchanged;
  omitted create body becomes an empty description.
- Edit omission preserves title, explicit null/blank is `400`, and a nonblank value
  replaces title exactly; title-only edit is valid.
- Feed pins/posts, detail, trending, controller hydration, SQL and serde/cache fixtures
  all carry required title plus description-only body.
- Authentication/extraction tests preserve unauthenticated `401` precedence and
  distinguish authenticated validation `400` from whole-request `413`.

## 17. Rollout

1. Recheck migration numbering against the latest migrations `origin/v2`; while both
   files remain unpublished, qualify and explicitly approve their one ordered PR.
2. Notify every frontend, mobile, bot, and direct API consumer that the title/body
   contract breaks immediately with no negotiation or compatibility response.
3. Gate every Dev Post read/write and both CMS controls, then drain every pre-title API
   node.
4. Record row count and available representative content shapes without inserting
   production samples; verify backups, WAL/free space, replica health, and feed/detail/
   trending/ranking TTLs are each `<= 60_000ms`.
5. Apply the audit migration followed by the title migration. The title migration's
   committed backfill is the point of no return for pre-title binaries.
6. Verify audit/title schema, unchanged row count, available representative rows,
   disposable first-LF/CRLF fixtures, audit preservation, and replica convergence.
7. Deploy only title-aware API nodes and verify feed v3, detail v2, trending v2, and
   generation-scoped ranking behavior on every node.
8. Delete exactly known `REDIS_KEY_PREFIX`-prefixed global/token feed legacy/v2/v3,
   detail legacy/v2, and trending legacy/v2 keys from controlled identifiers, then
   increment the exactly prefixed `devpost:ranking:generation`. Unknown keys expire
   naturally; never enumerate Redis.
9. Run authenticated create/edit/read/title-validation/auth-precedence smoke tests,
   then enable the breaking Dev Post contract and CMS controls.
10. Monitor create/edit `400`/`413`, endpoint 4xx/5xx, replica lag, cache PTTL/late
    fills, old-schema access, invalidation/generation warnings, audit insert and commit
    reconciliation/`outcome_unknown`, and lock waits/deadlocks.

The CMS frontend must treat `204` with an empty body as success and must not attempt JSON decoding.

## 18. Rollback

- Before the title migration commits, abort or roll back its transaction, keep traffic
  gated, and restore the pre-title binary/traffic only after confirming the old schema
  and payload contract remain intact.
- After the title migration commits, never run a pre-title binary, concatenate title
  back into body, drop the title column, or restore a compatibility response. Keep all
  Dev Post traffic gated and roll forward with a title-aware corrective binary.
- CMS controls alone may be disabled safely. Already committed deletes, restores, and
  audits remain authoritative; never drop `dev_post_moderation_log` or its data.
- Exact v3/v2 payload and generation keys expire naturally. Accept the bounded TTL and
  replica-lag contract rather than scanning Redis.
- Re-enabling after a title-aware corrective deployment repeats the complete old-node
  drain, replica/TTL check, exact known-key deletion, generation increment, smoke-test,
  and enable sequence. A prior run is not reusable evidence.

The audit migration needs no down migration. The title backfill is deliberately
roll-forward-only after its transaction commits.
