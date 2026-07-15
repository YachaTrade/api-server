# Task 9 report

## RED

Reviewed the moderation transaction path against the relation-preservation requirements. The restore branch deleted `dev_post_pin` before checking whether the post was actually deleted, so an idempotent restore could destroy an active post's pin. This was corrected so pin deletion occurs only during a real deleted → active transition; no historical pin is recreated.

## GREEN

Changed `src/controllers/dev_post/moderation.rs` so restore is conditional on `was` and remains idempotent. Existing transactional writes preserve images, polls/options, likes, votes, title, and body; orphan-token restores continue to return conflict and all relation/pin failures roll back with the surrounding transaction.

## Tests / results

`cargo test --lib controllers::dev_post::tests::delete_restore --no-default-features` — compilation reached existing stale assertions in `pin_feed_tests.rs` that still compare `CommitOutcome<PostMutationContext>` with `&str` (E0369). No test execution occurred. The failure is pre-existing API migration fallout and unrelated to this one-line restore fix.

## Commit

`338a00149536553efbef8c0a1c73082162188889 fix(dev-post): preserve pins on idempotent restore`

## Concerns

The worktree currently contains other moderation/title commits and the stale test compile errors above; those should be reconciled by the owning integration task before merge.

## Fix wave

Updated the two stale `pin_feed_tests` assertions to match `CommitOutcome::Committed(context)`, asserting `token_id` and `changed`, while treating `Unknown` as a failure. `cargo fmt --all -- --check` and the `controllers::dev_post::pin_feed_tests` target now compile successfully (tests require the configured SQLx database).
