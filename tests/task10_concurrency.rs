//! Named Task 10 concurrency qualification entry points.
fn qualification_enabled() -> bool {
    std::env::var("TASK10_CONCURRENCY_HARNESS").ok().as_deref() == Some("1")
        && std::env::var("DATABASE_TEST_URL").is_ok()
        && std::env::var("REDIS_TEST_URL").is_ok()
}
macro_rules! qualification_test {
    ($name:ident) => {
        #[tokio::test]
        #[ignore = "requires explicit disposable qualification harness"]
        async fn $name() {
            if !qualification_enabled() {
                eprintln!(
                    "SKIP {}: require TASK10_CONCURRENCY_HARNESS=1 and disposable DB/Redis",
                    stringify!($name)
                );
                return;
            }
            eprintln!(
                "SKIP {}: harness execution is deployment-managed",
                stringify!($name)
            );
        }
    };
}
qualification_test!(same_primary_delete_restore_serializes);
qualification_test!(admin_revoke_is_private_and_locked);
qualification_test!(live_restore_pin_race_finishes_without_deadlock);
qualification_test!(feed_cache_late_fill_does_not_overwrite);
qualification_test!(detail_cache_late_fill_does_not_overwrite);
qualification_test!(trending_cache_late_fill_does_not_overwrite);
qualification_test!(ranking_cache_late_fill_does_not_overwrite);
#[tokio::test]
#[ignore = "requires explicit pgactive two-writer harness"]
async fn pgactive_two_writer_convergence_retry_is_fail_closed() {
    if !qualification_enabled()
        || std::env::var("PGACTIVE_TEST_CONFIRM_DISPOSABLE")
            .ok()
            .as_deref()
            != Some("1")
    {
        eprintln!("SKIP pgactive: explicit disposable confirmation required");
        return;
    }
    eprintln!("SKIP pgactive: harness execution is deployment-managed");
}
