# CMS Dev Post moderation

`DELETE /cms/dev-post/{post_id}` returns 204/400/401/403/404/500. `POST /cms/dev-post/{post_id}/restore` returns 204/400/401/403/404/409/500. Both 204 responses have no body. Requests carry no request body; admin authentication uses the session extension and EIP-55 canonical address.

## Runbook

1. Publish/apply audit-schema readiness and notify every consumer of the immediate break.
2. Gate all Dev Post reads/writes and CMS controls, including direct API callers; drain every pre-title node.
3. Record row count and available samples (never insert production samples); verify backups, WAL/free space, replica health, and all four TTLs are `<=60_000ms`.
4. Apply migration `0041`. Its commit is the point of no return for pre-title binaries.
5. Verify title schema, row count, representative available rows, disposable-fixture shapes, audit schema, and replica convergence.
6. Deploy the title-aware API only; verify feed v3/detail v2/trending v2/ranking generation code on every node.
7. Delete exactly known prefixed global feed legacy/v2 keys, controlled-input token/detail keys, and trending legacy/v2 keys; increment the exact prefixed generation key. Unknown old keys expire naturally; never enumerate.
8. Run authenticated create/edit/read/title-validation/auth-precedence smoke tests, then enable the breaking contract and CMS controls.
9. Monitor create/edit 400/413, replica lag, cache PTTL/late fills, old-schema cache access, invalidation/generation warnings, lock/deadlock, audit/reconciliation/outcome-unknown, and title/body rendering.

Admin-first lock order is deterministic. Audit writes are idempotent; reconcile commit outcomes and retry outcome-unknown. Content and relationships are preserved, with conditional pin restore. pgactive limitations apply. Cache families are global feed, token feed, detail, and trending; TTL and late-fill bounds are each `<=60_000ms`.

Rollback before the title migration commit: abort/rollback the transaction and restore old binary/traffic. After commit, never run a pre-title binary, concatenate body, drop title, or restore a compatibility response. Keep traffic gated and roll forward with a title-aware corrective binary. CMS controls alone may be disabled safely while audit/content remain authoritative.
