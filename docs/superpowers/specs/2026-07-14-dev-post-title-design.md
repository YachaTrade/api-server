# Dev Post Explicit Title Design

> [!IMPORTANT]
> **APPROVED DECISION RECORD — EXECUTE ONLY THE INTEGRATED PLAN.** The title/body
> decisions in this document remain authoritative. The approved combined implementation,
> including the one-PR path while both migrations are unpublished, is
> [`../plans/2026-07-14-cms-dev-post-moderation-and-title.md`](../plans/2026-07-14-cms-dev-post-moderation-and-title.md).

- Date: 2026-07-14
- Status: Approved decision record; superseded for execution by the integrated plan
- Integration base: `v2`
- Scope: Dev Post PostgreSQL schema and legacy backfill, create/edit/read contracts,
  Redis cache compatibility, OpenAPI and API docs, tests, coordinated rollout
- Related design: `docs/superpowers/specs/2026-07-14-cms-dev-post-moderation-design.md`

## 1. Summary

Dev Post currently stores a title and description together in one `body` string, using
the first newline as an implicit boundary. That representation cannot distinguish a
newline inside the title from the separator before the description.

Add an explicit `dev_post.title` column and make `dev_post.body` the description only.
New post creation requires a non-blank title. Editing can omit `title` to preserve it,
but cannot clear it. Reads return `title` and description-only `body` as separate fields.
The title's trimmed value is used only to decide whether it is blank; the original input,
including leading/trailing whitespace and internal newlines, is stored and returned
unchanged. There is no title-specific validation cap, OpenAPI declares no `maxLength`, and
the existing whole-request transport limit remains.

The migration performs a one-time best-effort split of legacy `body` values at the first
line break. This cannot recover historical titles that themselves contained a newline;
that ambiguity is intrinsic to the old representation and is accepted explicitly.

This is a breaking data-contract migration, not a rolling-compatible additive response
field. The database backfill changes the meaning and value of `body`, while old binaries
do not read `title`. Rollout therefore uses a controlled Dev Post maintenance window,
new cache namespaces, old-node drain, migration-before-new-binary ordering, and a guarded
feature re-enable.

The cutover is intentionally immediate. There is no capability header, client-version
negotiation, versioned HTTP endpoint, or compatibility response. Existing clients that
have not adopted the new contract are allowed to fail or misrender after the cutover;
Section 4.4 records that accepted risk explicitly.

## 2. Goals

- Store title and description independently in PostgreSQL.
- Permit exact titles containing newlines without inferring their boundary from `body`.
- Require a non-empty, non-whitespace title on every newly created post.
- Allow title-only posts; an omitted create `body` is stored as an empty description.
- Let edits update title, body, images, or any supported combination atomically.
- Preserve omitted title values on edit and reject attempts to clear a title.
- Return a required `title` field and a description-only `body` field from every Dev Post
  read surface, including feed pins, ordinary feed posts, detail, and trending.
- Backfill existing rows deterministically and document the historical ambiguity.
- Prevent old Redis payloads from being interpreted under the new response semantics.
- Extend write invalidation so create/edit and CMS moderation cannot leave stale title or
  body payloads in a public cache namespace.
- Reuse the moderation design's ranking generation instead of enumerating ranking keys.
- Keep CMS delete/restore behavior independent of post content and preserve both title and
  body byte-for-byte through moderation.

## 3. Non-goals

- A title-specific validation cap, OpenAPI `maxLength`, truncation rule, or UI line-count
  policy
- Removing leading/trailing whitespace from a valid title
- Forbidding internal newlines in a title
- Reconstructing the true boundary of an ambiguous historical multiline title
- A title search index, title filtering, or title-based ranking
- Changing image, poll, like, vote, pin authorization, or pagination contracts
- Parsing tweet URLs from the title; `tweet_url` remains derived from description `body`
- A new Dev Post endpoint or a versioned HTTP path
- A capability header, client-version negotiation, or compatibility response
- A CMS title/body editor
- Logging title or body in moderation audit or structured operational logs
- Automatically reversing the title backfill during an API rollback

## 4. Alternatives considered

### 4.1 Separate title and description — selected

Add `dev_post.title`, migrate `dev_post.body` to description-only content, and expose both
fields directly in requests and responses.

This removes the ambiguous delimiter from the canonical model. A new title may contain
any number of line breaks because its boundary is the JSON/SQL field itself. It is the
cleanest long-term contract, and it makes create, edit, storage, and rendering agree on
the same representation.

The cost is a breaking rollout: old API binaries and clients understand `body` as a
combined envelope. That cost is accepted and controlled operationally rather than kept
as permanent data duplication.

### 4.2 Add title metadata but keep combined body — rejected

This would preserve old clients by leaving `body = title + "\n" + description` and adding
`title` as duplicate metadata. A new client could remove the exact title prefix, even if
the title contained newlines.

The representation would still have two sources of truth. Every title edit would have to
rewrite a prefix inside `body`, drift would remain possible, and all future consumers
would need to remember that `body` is not actually the body. The compatibility benefit
does not justify permanent synchronization logic.

### 4.3 Add title and description while retaining legacy body — rejected

A transition response could expose `title`, `description`, and legacy combined `body`, or
the service could introduce v2 HTTP endpoints. This gives the strongest rolling-client
compatibility but duplicates payloads, schemas, cache representations, documentation,
and deprecation work.

The Dev Post feature can use a coordinated rollout, so maintaining two public contracts
is unnecessary.

### 4.4 Accepted breaking-client risk

The selected design deliberately does not detect or adapt to an old client:

- An old reader ignores the unknown response `title` and interprets description-only
  `body` as the old combined envelope. It can display the first description line as a
  false title and omit the actual title entirely.
- An authenticated old create request omits required `title` and receives `400`. An
  unauthenticated request still receives `401` before JSON extraction.
- An old edit request that sends only its legacy combined `body` can still satisfy the
  optional-title edit contract; the server will store that entire string as description.
  The API cannot distinguish this from an intentional new-client body edit without the
  rejected negotiation mechanisms.
- Once the title backfill commits, a pre-title binary cannot safely serve reads. This is
  the rollback point of no return described in Sections 12 and 13.

These are accepted product risks, not gaps to solve with a hidden fallback. Rollout must
notify frontend, mobile, bot, and direct API consumers, but it does not wait for every old
client to upgrade.

## 5. Data model and migration

### 5.1 Schema

Add this column after the permanent moderation-audit migration:

```sql
ALTER TABLE dev_post
    ADD COLUMN title TEXT NOT NULL DEFAULT '';
```

There is no database `CHECK (btrim(title) <> '')`. Legacy empty posts must remain
representable, direct pre-title writers and migration fixtures rely on the additive
default during rollout/testing, and request validation owns the new-write invariant.
No index is needed because title is neither filtered nor sorted.

The application invariant after rollout is:

- API-created rows have a non-blank title;
- `title` stores exactly the accepted request string;
- `body` stores only the description and is never prefixed with title;
- legacy rows may retain an empty title only when the old body was empty or began with a
  line break.

### 5.2 Legacy backfill

The title migration transforms every row using the first legacy line break:

| Legacy `body` | Backfilled `title` | Backfilled `body` |
|---|---|---|
| `"Title\nDescription"` | `"Title"` | `"Description"` |
| `"Title\nLine 1\nLine 2"` | `"Title"` | `"Line 1\nLine 2"` |
| `"Title"` | `"Title"` | `""` |
| `""` | `""` | `""` |
| `"\nDescription"` | `""` | `"Description"` |

The delimiter is the first LF (`\n`). If that LF is immediately preceded by CR (`\r`),
consume the CRLF pair as one delimiter; a bare CR is ordinary content. Do not otherwise
trim or normalize either side. All subsequent line breaks remain in the description.

If a historical title actually contained a line break, the migration will interpret its
first line break as the title/description delimiter. There is no information in the old
row that can disambiguate that case, so the migration must not claim exact recovery.

The schema change and backfill run in one numbered migration transaction. The migration
runner's applied-version record is the exactly-once guard for the content split; do not
publish the backfill as a standalone rerunnable operator script. Re-running the split
would corrupt an already separated body.

### 5.3 Numbering and repository workflow

`migrations/` is a separate repository submodule. Recheck its latest `origin/v2`
immediately before creating SQL and again before publishing. When neither approved
migration exists there at execution time, create these ordered files together:

```text
0040_dev_post_moderation_log.sql
0041_dev_post_title.sql
```

The audit migration is first and the title schema/backfill migration is second. Put both
in one migrations feature branch and one Draft PR, run their executable qualification
against an explicitly disposable PostgreSQL database, review both ordered files, report
the exact pushed head and results, and pause for explicit user approval. Only after that
approval may the PR transition to ready and squash-merge without `--delete-branch`.
Synchronize migrations `v2` afterward and make the API gitlink and two separate numbered
`migrations-test/` symlinks point to that final squash commit.

The collision fallback is fixed: any filename already in `origin/v2` is published and immutable; only unpublished remaining work moves to the next canonical number. Preserve
the dependency order of moderation audit before title backfill, ignore the special
`1000_delete.sql` when sequencing, and update both decision records, the integrated plan,
symlink names, tests, and PR text in a documentation-only commit before resuming. Never
rename, amend, reuse, or reorder a published file.

### 5.4 Migration operational characteristics

Adding the column is additive, but splitting title from body updates every existing
`dev_post` row. Before rollout, record row count, estimate update duration on a production-
like dataset, verify free space/WAL capacity, and confirm backup/restore readiness. The
Dev Post maintenance window must cover the backfill and binary transition. After the
migration, verify total row count is unchanged and inspect representative rows that exist.

Never insert synthetic sample posts into production. If production contains no rows, or
does not contain each backfill shape in Section 5.2, record that fact and prove the missing
cases with the executable disposable-database fixtures before approval. The absence of a
production sample does not weaken the fixture contract.

## 6. HTTP and type contracts

No endpoint count or path changes. Existing Dev Post operations remain 13, and the
moderation feature adds its separate 2 CMS operations.

### 6.1 Create request

`CreateDevPostRequest` becomes conceptually:

```rust
pub struct CreateDevPostRequest {
    pub token_id: String,
    pub title: String,
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
    pub poll: Option<CreatePollRequest>,
}
```

Rules:

- `title` is required in JSON. Missing or `null` title returns `400`.
- `title.trim().is_empty()` returns `400`.
- A valid title is stored exactly as received. `trim()` is validation-only.
- There is no title-specific validation cap; OpenAPI declares no `maxLength`, and the
  existing whole-request transport limit remains.
- Internal `\n` and `\r\n` sequences are valid title content.
- Omitted `body` is stored as `""`; a provided body is stored exactly as received.
- A title-only post is valid. The old body/image/poll at-least-one rule is superseded by
  the required non-blank title.
- Existing token EIP-55 normalization and creator authorization remain unchanged.

The protected router must run `authenticate_user` before consuming or validating the JSON
body. For an authenticated request, a custom request extractor or explicit
`JsonRejection` mapping converts missing/null/blank title and JSON syntax/type failures to
the repository's JSON error shape with HTTP `400`, not Axum's default `422`. A body-limit
rejection remains `413`; it must not be collapsed into `400`. For the same malformed,
missing-title, or over-limit request without a valid session, middleware wins and returns
`401` before body extraction. OpenAPI marks create title as required and non-null.

There is no title-specific validation cap and OpenAPI declares no `maxLength`; the existing
whole-request HTTP/JSON transport limit remains. A title within that request limit is
accepted regardless of character count, while a request exceeding it returns `413`
according to the router/body-limit contract.

### 6.2 Edit request

The edit wire contract is an exact tri-state even though OpenAPI exposes an optional
non-null string:

- Omitted `title`: preserve the stored title.
- Present non-blank string: replace the title with the exact original string.
- Present `null`, `""`, or whitespace-only string: `400`; title cannot be cleared.
- Title-only edit is valid.
- If title, body, and image fields are all omitted, retain the existing `400 Nothing to
  update` behavior.
- A successful title-only edit updates `updated_at`, sets `edited_at`, and returns
  `is_edited: true`, just like a body/image edit.
- Body omission and image replacement retain their existing semantics.

Plain Rust `Option<String>` is insufficient because Serde maps both omission and explicit
`null` to `None`. Use a presence-aware wire representation (for example an internal
`Omitted | Null | Value(String)` field with `#[serde(default)]` plus a custom deserializer)
so `null` reaches validation and returns `400` rather than preserving the title.

Utoipa must describe the public field, not the internal enum: `title` is absent from the
request schema's `required` array, its value schema is `type: string`, and it has neither
`nullable: true` nor a JSON Schema union containing `null`. A focused generated-OpenAPI
test verifies all three properties. Runtime serde tests independently prove the three wire
states.

### 6.3 Response

`DevPostResponse` gains a required string field:

```rust
pub struct DevPostResponse {
    // existing id/token/author fields
    pub title: String,
    pub body: String,
    // existing tweet/images/poll/like/timestamp fields
}
```

`body` now always means description only. This applies uniformly to:

- `GET /dev-post` `pin`;
- `GET /dev-post` ordinary `posts`;
- `GET /dev-post/{post_id}`;
- `GET /dev-post/trending`;
- create and edit read-after-write responses;
- cached viewer-neutral `FeedBase` and detail/trending payloads.

The title is required in the response even for migrated empty legacy posts; those posts
return `title: ""` and `body: ""`. The API does not concatenate the fields for old clients.
`tweet_url` continues to parse the description `body` only.

### 6.4 Error behavior

Title validation uses the existing JSON error envelope and `AppError::BadRequest` mapping.
It introduces no new status code. Authentication precedes JSON extraction (`401`), an
authenticated invalid title/JSON value is `400`, and the transport-size limit remains
`413`. After successful extraction/validation, creator/author authorization, missing
token/post, deleted-post, image, and poll errors retain their current precedence.

## 7. Controller and SQL behavior

### 7.1 Create

The create transaction inserts both content fields:

```sql
INSERT INTO dev_post (token_id, author, title, body)
VALUES ($1, $2, $3, $4)
RETURNING id;
```

Title and body are written in the same transaction as images and optional poll rows.
No concatenation or splitting occurs for a new request.

### 7.2 Edit

The author check and write-pool transaction stay unchanged. The update applies each
present field without changing omitted fields. Title and body updates, image replacement,
and `updated_at`/`edited_at` changes commit atomically. A failed validation performs no SQL;
a later SQL failure rolls back all content/image changes.

### 7.3 Hydration

The core Dev Post row query selects `dp.title` and maps it directly to
`DevPostResponse.title`. It maps `dp.body` directly to description-only response `body`.
Hydration performs no legacy split after the migration. This makes the completed database
backfill the single compatibility boundary instead of hiding mixed formats in read code.

All test constructors and cached serde fixtures must include title explicitly. A missing
title in an old cache payload must be treated as an old-schema cache miss, not synthesized
from cached body.

## 8. Redis cache design

### 8.1 Versioned payload namespaces

The response schema and the meaning of `body` change together. New code must not read
payloads written under the old shape. Introduce these exact new key families:

```text
devpost:feed:v3:{scope}
devpost:detail:v2:{post_id}
devpost:trending:v2
```

Every key continues to use the configured `REDIS_KEY_PREFIX`. The new reader/writer uses
only these versions. Legacy feed keys (`devpost:feed:{scope}` and
`devpost:feed:v2:{scope}`), legacy detail (`devpost:detail:{post_id}`), and legacy
trending (`devpost:trending`) are never fallback read sources.

Old payloads expire naturally. Rollout and mutation invalidation delete exact known keys;
no `KEYS`, `SCAN`, wildcard deletion, prefix-wide deletion, or `FLUSHALL` is allowed.

### 8.2 Exact invalidation

Shared invalidation helpers must independently attempt every applicable old and new exact
key so rollback residue cannot reappear:

| Mutation | Exact payload invalidation | Ranking generation |
|---|---|---|
| Create | global feed legacy/v2/v3; token feed legacy/v2/v3; trending legacy/v2 | `INCR` |
| Edit title/body/images | global feed legacy/v2/v3; token feed legacy/v2/v3; detail legacy/v2; trending legacy/v2 | no bump |
| Author delete | full global/token feed, detail, trending families | `INCR` |
| CMS delete/restore, including no-op success | full global/token feed, detail, trending families | `INCR` |
| Pin/unpin | token feed legacy/v2/v3 | no bump |

Create changes ranking `post_count` and `last_posted_at`, so it bumps the generation.
Editing title/body/images does not change a ranking input, so it does not. The existing
like/unlike/vote cache policy is not broadened by this title design.

Redis remains advisory. A cache delete or generation bump failure never reverses a
confirmed PostgreSQL create/edit/moderation result. Failures are logged without title or
body. The affected payload may remain stale until its configured TTL.

### 8.3 Ranking generation

Use the moderation design's captured-generation algorithm and keys:

```text
devpost:ranking:generation
devpost:ranking:v2:{generation}:{page}:{limit}
```

Do not add a title-specific ranking namespace. `RankingResponse` contains aggregate/token
rows rather than Dev Post title/body content, so the schema remains unchanged. A create,
delete, or restore bumps the generation because the aggregate changes. Title/body edits
do not.

### 8.4 Accepted replica and cache-fill races

Exact invalidation is best-effort cache coherence, not strong read-after-write
consistency. A feed/detail/trending cache miss can read a stale replica value before or
after a PostgreSQL commit, pause, and write that stale value into the new cache namespace
after the mutation's exact `DEL` has already completed. A direct uncached public read can
also observe replica lag. The title feature does not add distributed locks or route all
public reads to the primary.

The accepted stale upper bound is the TTL measured from the late stale cache fill, not
from the database commit. For this release, configured Dev Post feed, detail, trending,
and ranking expirations must each be no greater than `60_000ms`; current defaults are
`60_000ms`. Startup configuration validation or an equivalent deployment gate must reject
or block enabling the feature when any value exceeds that bound. Replica-only staleness is
bounded operationally by observed replica lag rather than Redis TTL, so rollout and
monitoring require a healthy lag threshold.

Ranking avoids the analogous old-fill overwrite when invalidation succeeds: a reader that
captured the old generation may populate only its old generation key after an `INCR`; new
readers use the new generation. A failed generation bump retains the existing
best-effort/TTL limitation.

Deterministic tests must use barriers or injectable ordering seams, not timing luck, to
force `stale read -> mutation commit and invalidation -> stale cache SET`. Assert that the
result may be stale, that its Redis `PTTL` is positive and no greater than the configured
test TTL, and that PostgreSQL remains authoritative. Use a short isolated test TTL or an
exact-key cleanup rather than sleeping for the production TTL. Separate ranking tests
prove a late old-generation SET is invisible after a successful bump.

## 9. CMS moderation interaction

CMS delete and restore remain content-blind:

- delete changes `deleted_at`, removes the applicable pin, and writes its audit row;
- restore changes `deleted_at` according to the approved idempotency rules and does not
  restore a historical pin;
- neither operation updates `title` or `body`;
- audit rows and structured logs never contain title or body;
- actual and no-op CMS success uses the expanded v3/v2 cache invalidation families;
- author delete uses the same expanded invalidation after its existing authorization and
  non-idempotent state transition.

Moderation relationship, rollback, same-primary lock-order, commit-reconciliation, and
pgactive guarantees remain as specified in the moderation design. Add preservation tests
that capture both title and body before delete/restore and assert exact equality after.

### 9.1 Moderation-document execution gate

The documentation gate is satisfied only when the moderation decision record and this
decision record are committed with execution banners, the historical 16-task
moderation-only plan is permanently marked non-executable, and both independent document
reviews report no Critical or Important finding. Before that documentation-only commit,
no migration branch, production Rust change, or executable title test may be created.

After the gate, neither decision record nor the historical plan is a task runner. The
sole executable source is the linked integrated moderation-and-title plan. It owns the
one-PR path for both migrations while unpublished, every TDD/review/approval pause, the
immediate title/body cutover, and the Draft API PR handoff.

Every direct `dev_post` fixture must either provide an intentional title or explicitly
assert why the legacy/default-empty title is under test. Cache fixtures must use the new
response field and exact versioned keys. The revised plan must be reviewed for internal
consistency before implementation resumes.

## 10. OpenAPI and documentation

Update the registered schemas and examples for:

- `CreateDevPostRequest`: required, non-null `title`; optional description `body`;
- `EditDevPostRequest`: optional, non-null `title`; omission preserves it;
- `DevPostResponse`: required `title`; description-only required `body`;
- all nested feed, pin, trending, and cached examples that embed `DevPostResponse`.

Document `400` for missing/null/blank create title and null/blank edit title. Document
that title preserves whitespace/newlines exactly after non-blank validation, has no
title-specific validation cap or OpenAPI `maxLength`, and remains subject to the existing
whole-request transport limit. Remove all examples and prose that describe title as the
first line of body or body as title plus description.

Swagger UI, OpenAPI JSON, and Scalar continue to expose the same 13 Dev Post operations
and the separate 2 CMS operations. Contract tests must inspect component `required`,
property type/nullability, examples, and operation counts.

Update at least:

- `docs/dev-post-api.md`;
- `docs/V2_API_CHANGES.md`;
- the CMS moderation operational document created by its implementation plan;
- OpenAPI assertions in `src/main.rs`.

## 11. Test design

### 11.1 Migration tests

Use an executable disposable PostgreSQL schema contract to prove:

- `title` exists as `TEXT NOT NULL DEFAULT ''` and has no search index;
- legacy `Title\nDescription`, multi-line description, title-only, empty, and leading-line-
  break rows match the table in Section 5.2;
- CRLF is consumed as one delimiter without trimming other content;
- row IDs/count and related images/polls/likes/votes/pins are unchanged;
- moderation audit schema from the immediately preceding migration still passes;
- a second content split is not present in any later migration;
- while both files are unpublished, the tested audit/title migrations are ordered in
  one migrations branch/PR and the exact reviewed head is explicitly approved;
- the API gitlink after the one squash merge points to the final migrations `origin/v2`
  commit containing both files, and the immutable-publication fallback changed no
  published filename.

### 11.2 Validation and serialization tests

Cover:

- create missing/null/empty/whitespace title returns `400`;
- create title-only succeeds with empty body;
- internal-newline and leading/trailing-whitespace title round-trips exactly;
- a deliberately long title within the whole-request transport limit is accepted, proving
  no accidental title-specific validation cap, and OpenAPI has no `maxLength`;
- edit omission preserves title;
- edit null/empty/whitespace title returns `400`;
- title-only edit sets `is_edited` and preserves body/images;
- edit serde distinguishes omitted, explicit null, and string without conflation;
- generated OpenAPI describes edit title as optional and non-null while create/response
  title is required and non-null;
- serde/OpenAPI require response title and reject old cached payloads as cache misses;
- a title well within the request transport limit has no title-specific length rejection,
  while an authenticated over-limit request remains `413`.

Real-router tests must cover middleware/extractor precedence. For missing/null/blank title
and malformed JSON, a valid session receives the exact `400` error envelope; without a
valid session the same requests receive `401` before extraction. Repeat the precedence
assertion for an over-limit body (`401` unauthenticated, `413` authenticated).

### 11.3 Controller and service tests

Cover create SQL, all edit field combinations, atomic rollback, write-pool read-after-write,
feed pin/ordinary posts, detail, trending, and description-only tweet parsing. Verify exact
old/new cache invalidation for create/edit/pin/unpin/author delete/CMS delete/restore and
the create/delete/restore ranking generation policy. Redis failure tests must prove the
database result remains successful. Barrier-driven race tests must prove the accepted
late stale fill and its TTL bound, plus old-generation ranking isolation, without relying
on random task scheduling or a production-length sleep.

### 11.4 CMS tests

Extend moderation preservation, rollback, idempotency, concurrency, and pgactive fixtures
with explicit titles. Confirm delete/restore never changes either content column and never
places either value in audit/log output.

### 11.5 Documentation and rollout tests

Assert that generated OpenAPI and checked-in references use separate title/body semantics,
retain 13+2 operation counts, list the breaking rollout gate, and contain no stale
`title\ndescription` request/response example. Add negative assertions that no capability
header, client-version negotiation, v2 endpoint, or compatibility response was introduced.
The rollout document must state old-read misrendering, authenticated old-create `400`,
legacy edit ambiguity, the migration point of no return, transport-limit distinction, and
the production-no-sample fixture rule.

## 12. Coordinated rollout

This change cannot use a normal mixed old/new API rollout. The title migration rewrites
`body`, and an old binary would return description without the new title. Conversely, the
new binary selects `title` and cannot run before the schema exists.

Required order:

1. While both files remain unpublished, create, qualify, review, explicitly approve,
   squash-merge, and synchronize the ordered audit/title migrations in one migrations
   PR. Apply audit-schema readiness, but do not execute title backfill before the
   maintenance gate. If execution-time `origin/v2` has changed, follow Section 5.3's
   immutable-publication fallback before resuming.
2. Announce the immediate breaking cutover to frontend/mobile/bot/direct API consumers.
   Consumer readiness is recommended but is not a compatibility gate.
3. Start the maintenance gate for all Dev Post reads/writes and CMS controls, including
   direct API traffic, and stop every pre-title API node from serving Dev Post routes.
4. Record row count and available production samples; use disposable fixtures for absent
   shapes. Confirm backup/restore readiness, replica health, and all four Dev Post cache
   TTLs at or below `60_000ms`.
5. Run the title migration transaction. Its successful commit is the point of no return:
   from then on, a pre-title binary is forbidden against this database.
6. Verify title schema, fixture-backed backfill contract, representative production rows,
   unchanged row count, and replica convergence.
7. Deploy only the title-aware API binary. Confirm every node uses feed v3, detail v2,
   trending v2, and ranking generation v2 code.
8. Delete the exactly prefixed, statically known legacy/v2 global feed keys. Delete old
   token-feed and detail keys only when a token/post identifier is already available from
   a controlled rollout input; do not enumerate Redis to discover them. Unknown old
   payloads expire naturally because new readers ignore their namespaces.
9. Delete the exact legacy and v2 trending keys and atomically increment the exact prefixed
   ranking generation key.
10. Run authenticated create/edit/read smoke tests covering a title with an internal
   newline, empty description, `400` title validation, and auth-before-JSON precedence.
11. Enable the new Dev Post contract immediately, without capability negotiation or a
   compatibility response. Old readers may misrender and old create clients receive `400`.
12. Enable CMS controls only after the moderation design's admin/audit/cache checks pass.
13. Monitor create/edit `400` rates, migration/replica lag, cache PTTL/late fills, old-schema
    cache access, Redis invalidation warnings, ranking generation errors, CMS outcomes, and
    title/body rendering.

The operational mechanism for disabling Dev Post traffic may be a gateway route gate,
maintenance response, or deployment feature flag, but it must cover direct API callers,
not only hide frontend buttons.

## 13. Rollback

Before the title-migration transaction commits, aborting the maintenance window is safe:
roll back the transaction, leave the old binary in place, and re-enable the old contract.

After that commit, rollback to a pre-title binary or response is not supported. Keep Dev
Post traffic disabled and ship a corrective title-aware roll-forward binary. Preserve
`dev_post.title`, separated `body`, moderation audit data, and new Redis namespaces. Do not
concatenate title back into database body, drop the title column, or introduce an emergency
compatibility response; none can reconstruct every ambiguous historical row exactly.

Before re-enabling after a corrective build, repeat replica checks, exact trending
deletion, ranking generation increment, and smoke tests. Rolling back only CMS controls is
safe; audit rows and title/body data remain authoritative.

## 14. Acceptance criteria

- New create requests cannot succeed without a non-blank title and return `400` for all
  specified invalid title forms.
- New/edit titles preserve exact accepted bytes and accept internal newlines with no
  title-specific validation cap or OpenAPI `maxLength`; the existing whole-request
  transport limit remains.
- PostgreSQL and every API response store/return title and description separately.
- Legacy rows match the deterministic first-line backfill table, with the historical
  multiline-title limitation documented.
- Every read surface and cache payload returns required `title` plus description-only
  `body`; no new code reads old payload namespaces.
- Create/edit/moderation invalidation and ranking generation follow Section 8 exactly,
  without Redis enumeration.
- CMS delete/restore preserve title/body and never audit or log content.
- OpenAPI/docs expose the updated fields without changing the 13 Dev Post + 2 CMS operation
  count.
- Rollout prevents a pre-title binary from serving after body backfill, and rollback never
  deploys that binary directly against the separated schema.
