use super::DevPostController;
use crate::db::postgres::PostgresDatabase;
use crate::result::AppError;
use std::sync::Arc;

const TOKEN: &str = "0x0000000000000000000000000000000000007777";
const OTHER_TOKEN: &str = "0x0000000000000000000000000000000000017777";
const CREATOR: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
const VIEWER_A: &str = "0x000000000000000000000000000000000000Aa01";
const VIEWER_B: &str = "0x000000000000000000000000000000000000aA02";

fn controller(pool: sqlx::PgPool) -> DevPostController {
    DevPostController::new(Arc::new(PostgresDatabase {
        write_pool: pool.clone(),
        read_pool: pool,
    }))
}

async fn seed_token(pool: &sqlx::PgPool, token_id: &str) {
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'Token', 'TKN', 'img', $2, 0, '0xhash', 0)",
    )
    .bind(token_id)
    .bind(CREATOR)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_post(pool: &sqlx::PgPool, token_id: &str, title: &str, body: &str) -> i64 {
    // Age existing posts for this token back a day first so `posted_on`
    // (generated from `created_at`) recomputes off today, freeing today's
    // slot under `uq_dev_post_token_daily` for the row inserted below.
    sqlx::query(
        "UPDATE dev_post SET created_at = created_at - INTERVAL '1 day' WHERE token_id = $1",
    )
    .bind(token_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, title, body) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(token_id)
    .bind(CREATOR)
    .bind(title)
    .bind(body)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_poll(pool: &sqlx::PgPool, post_id: i64) {
    sqlx::query(
        "INSERT INTO dev_post_poll (post_id, closes_at) \
         VALUES ($1, NOW() + INTERVAL '1 day')",
    )
    .bind(post_id)
    .execute(pool)
    .await
    .unwrap();
    for (position, label) in [(1_i16, "A"), (2_i16, "B")] {
        sqlx::query(
            "INSERT INTO dev_post_poll_option (post_id, position, label) VALUES ($1, $2, $3)",
        )
        .bind(post_id)
        .bind(position)
        .bind(label)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn pin_and_posts_share_viewer_overlay_without_cross_viewer_leak(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let ordinary = seed_post(&pool, TOKEN, "Ordinary title", "ordinary").await;
    let pin = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    seed_poll(&pool, ordinary).await;
    seed_poll(&pool, pin).await;
    for (post_id, viewer, option) in [
        (pin, VIEWER_A, 1_i16),
        (ordinary, VIEWER_A, 2_i16),
        (pin, VIEWER_B, 2_i16),
    ] {
        sqlx::query("INSERT INTO dev_post_like (post_id, account_id) VALUES ($1, $2)")
            .bind(post_id)
            .bind(viewer)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO dev_post_poll_vote (post_id, account_id, option_position) \
             VALUES ($1, $2, $3)",
        )
        .bind(post_id)
        .bind(viewer)
        .bind(option)
        .execute(&pool)
        .await
        .unwrap();
    }
    let controller = controller(pool);
    controller.pin_post(pin, CREATOR).await.unwrap();

    let anonymous = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert!(!anonymous.pin.as_ref().unwrap().liked_by_me);
    assert_eq!(
        anonymous
            .pin
            .as_ref()
            .unwrap()
            .poll
            .as_ref()
            .unwrap()
            .my_vote_option,
        None
    );

    let viewer_a = controller
        .get_feed(Some(TOKEN), 1, 10, Some(VIEWER_A))
        .await
        .unwrap();
    assert!(viewer_a.pin.as_ref().unwrap().liked_by_me);
    assert_eq!(
        viewer_a
            .pin
            .as_ref()
            .unwrap()
            .poll
            .as_ref()
            .unwrap()
            .my_vote_option,
        Some(1)
    );
    assert!(viewer_a.posts[0].liked_by_me);
    assert_eq!(
        viewer_a.posts[0].poll.as_ref().unwrap().my_vote_option,
        Some(2)
    );

    let viewer_b = controller
        .get_feed(Some(TOKEN), 1, 10, Some(VIEWER_B))
        .await
        .unwrap();
    assert!(viewer_b.pin.as_ref().unwrap().liked_by_me);
    assert_eq!(
        viewer_b
            .pin
            .as_ref()
            .unwrap()
            .poll
            .as_ref()
            .unwrap()
            .my_vote_option,
        Some(2)
    );
    assert!(!viewer_b.posts[0].liked_by_me);
    assert_eq!(
        viewer_b.posts[0].poll.as_ref().unwrap().my_vote_option,
        None
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn token_pages_return_pin_only_on_page_one_and_exclude_before_paging(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let oldest = seed_post(&pool, TOKEN, "Oldest title", "oldest").await;
    let pinned = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    let newest = seed_post(&pool, TOKEN, "Newest title", "newest").await;
    let controller = controller(pool);
    controller.pin_post(pinned, CREATOR).await.unwrap();

    let page_one = controller.get_feed(Some(TOKEN), 1, 1, None).await.unwrap();
    assert_eq!(page_one.pin.unwrap().id, pinned.to_string());
    assert_eq!(page_one.posts[0].id, newest.to_string());
    assert_eq!(page_one.total_count, 2);

    let page_two = controller.get_feed(Some(TOKEN), 2, 1, None).await.unwrap();
    assert!(page_two.pin.is_none());
    assert_eq!(page_two.posts[0].id, oldest.to_string());
    assert_eq!(page_two.total_count, 2);

    let page_three = controller.get_feed(Some(TOKEN), 3, 1, None).await.unwrap();
    assert!(page_three.pin.is_none());
    assert!(page_three.posts.is_empty());
    assert_eq!(page_three.total_count, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn pin_only_empty_full_and_past_end_pages_preserve_contract(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let controller = controller(pool.clone());
    let empty = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert!(empty.pin.is_none());
    assert!(empty.posts.is_empty());
    assert_eq!(empty.total_count, 0);

    let pin = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    controller.pin_post(pin, CREATOR).await.unwrap();
    let pin_only = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert_eq!(pin_only.pin.unwrap().id, pin.to_string());
    assert!(pin_only.posts.is_empty());
    assert_eq!(pin_only.total_count, 0);

    for index in 0..10 {
        seed_post(
            &pool,
            TOKEN,
            &format!("Ordinary title {index}"),
            &format!("ordinary-{index}"),
        )
        .await;
    }
    let full = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert!(full.pin.is_some());
    assert_eq!(full.posts.len(), 10);
    assert_eq!(full.total_count, 10);
    let past_end = controller.get_feed(Some(TOKEN), 4, 10, None).await.unwrap();
    assert!(past_end.pin.is_none());
    assert!(past_end.posts.is_empty());
    assert_eq!(past_end.total_count, 10);
    let selection = super::pin::select_token_feed(controller.db.get_read_pool(), TOKEN, 10, 30)
        .await
        .unwrap();
    assert_eq!(selection.active_pin_id, Some(pin));
}

#[sqlx::test(migrations = "./migrations")]
async fn global_feed_ignores_pin_state(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let pin = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    let ordinary = seed_post(&pool, TOKEN, "Ordinary title", "ordinary").await;
    let controller = controller(pool);
    controller.pin_post(pin, CREATOR).await.unwrap();

    let global = controller.get_feed(None, 1, 10, None).await.unwrap();
    let ids: Vec<String> = global.posts.into_iter().map(|post| post.id).collect();
    assert!(global.pin.is_none());
    assert_eq!(global.total_count, 2);
    assert_eq!(ids, vec![ordinary.to_string(), pin.to_string()]);
    let global_page_two = controller.get_feed(None, 2, 1, None).await.unwrap();
    assert!(global_page_two.pin.is_none());
    assert_eq!(global_page_two.posts[0].id, pin.to_string());
    assert_eq!(global_page_two.total_count, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_or_cross_token_mapping_never_emits_or_hides_wrong_post(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    seed_token(&pool, OTHER_TOKEN).await;
    let x_post = seed_post(&pool, TOKEN, "X title", "x").await;
    let y_post = seed_post(&pool, OTHER_TOKEN, "Y title", "y").await;
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(TOKEN)
        .bind(y_post)
        .execute(&pool)
        .await
        .unwrap();
    let controller = controller(pool.clone());

    let x = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert!(x.pin.is_none());
    assert_eq!(x.posts[0].id, x_post.to_string());
    let y = controller
        .get_feed(Some(OTHER_TOKEN), 1, 10, None)
        .await
        .unwrap();
    assert!(y.pin.is_none());
    assert_eq!(y.posts[0].id, y_post.to_string());

    sqlx::query("DELETE FROM dev_post_pin")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind(TOKEN)
        .bind(x_post)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE dev_post SET deleted_at = NOW() WHERE id = $1")
        .bind(x_post)
        .execute(&pool)
        .await
        .unwrap();
    let stale = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert!(stale.pin.is_none());
    assert!(stale.posts.is_empty());
    assert_eq!(stale.total_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn creator_transfer_keeps_old_pin_visible_and_excluded(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let ordinary = seed_post(&pool, TOKEN, "Ordinary title", "ordinary").await;
    let old_pin = seed_post(&pool, TOKEN, "Old pin title", "old pin").await;
    let controller = controller(pool.clone());
    controller.pin_post(old_pin, CREATOR).await.unwrap();
    let new_creator = "0xde709f2102306220921060314715629080e2fb77";
    sqlx::query("UPDATE token SET creator = $2 WHERE token_id = $1")
        .bind(TOKEN)
        .bind(new_creator)
        .execute(&pool)
        .await
        .unwrap();

    let feed = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    assert_eq!(feed.pin.unwrap().id, old_pin.to_string());
    assert_eq!(feed.posts[0].id, ordinary.to_string());
    assert_eq!(feed.total_count, 1);
    controller.unpin_post(old_pin, new_creator).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn replaced_and_unpinned_posts_reenter_newest_first_pagination(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let oldest = seed_post(&pool, TOKEN, "Oldest title", "oldest").await;
    let middle = seed_post(&pool, TOKEN, "Middle title", "middle").await;
    let newest = seed_post(&pool, TOKEN, "Newest title", "newest").await;
    let controller = controller(pool);

    controller.pin_post(middle, CREATOR).await.unwrap();
    controller.pin_post(newest, CREATOR).await.unwrap();
    let replaced = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    let replaced_ids: Vec<String> = replaced.posts.into_iter().map(|post| post.id).collect();
    assert_eq!(replaced.pin.unwrap().id, newest.to_string());
    assert_eq!(replaced_ids, vec![middle.to_string(), oldest.to_string()]);

    controller.unpin_post(newest, CREATOR).await.unwrap();
    let unpinned = controller.get_feed(Some(TOKEN), 1, 10, None).await.unwrap();
    let unpinned_ids: Vec<String> = unpinned.posts.into_iter().map(|post| post.id).collect();
    assert!(unpinned.pin.is_none());
    assert_eq!(
        unpinned_ids,
        vec![newest.to_string(), middle.to_string(), oldest.to_string()]
    );
    assert_eq!(unpinned.total_count, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_pinned_post_soft_deletes_and_unpins_in_one_commit(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let pin = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    let controller = controller(pool.clone());
    controller.pin_post(pin, CREATOR).await.unwrap();
    match controller.delete_post(pin, CREATOR).await.unwrap() {
        crate::controllers::dev_post::moderation::CommitOutcome::Committed(context) => {
            assert_eq!(context.token_id, TOKEN);
            assert!(context.changed);
        }
        crate::controllers::dev_post::moderation::CommitOutcome::Unknown { .. } => {
            panic!("delete commit outcome unknown")
        }
    }

    let state: (bool, i64) = sqlx::query_as(
        "SELECT deleted_at IS NOT NULL, \
         (SELECT count(*) FROM dev_post_pin WHERE post_id = $1) \
         FROM dev_post WHERE id = $1",
    )
    .bind(pin)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, (true, 0));
}

#[sqlx::test(migrations = "./migrations")]
async fn pin_delete_race_cannot_leave_deleted_post_pinned(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let post = seed_post(&pool, TOKEN, "Race title", "race").await;
    let pin_controller = controller(pool.clone());
    let delete_controller = controller(pool.clone());
    let (pin_result, delete_result) = tokio::join!(
        pin_controller.pin_post(post, CREATOR),
        delete_controller.delete_post(post, CREATOR)
    );
    match delete_result.unwrap() {
        crate::controllers::dev_post::moderation::CommitOutcome::Committed(context) => {
            assert_eq!(context.token_id, TOKEN);
            assert!(context.changed);
        }
        crate::controllers::dev_post::moderation::CommitOutcome::Unknown { .. } => {
            panic!("delete commit outcome unknown")
        }
    }
    match pin_result {
        Ok(token_id) => assert_eq!(token_id, TOKEN),
        Err(AppError::NotFound(message)) => assert_eq!(message, "Post not found"),
        result => panic!("unexpected concurrent pin result: {result:?}"),
    }
    let invalid: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM dev_post_pin dpp \
         JOIN dev_post dp ON dp.id = dpp.post_id \
         WHERE dp.deleted_at IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invalid, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_failure_rolls_back_unpin_and_soft_delete(pool: sqlx::PgPool) {
    seed_token(&pool, TOKEN).await;
    let pin = seed_post(&pool, TOKEN, "Pinned title", "pin").await;
    let controller = controller(pool.clone());
    controller.pin_post(pin, CREATOR).await.unwrap();
    sqlx::query(
        "CREATE FUNCTION fail_dev_post_soft_delete() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'forced delete failure'; END $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_dev_post_soft_delete_trigger \
         BEFORE UPDATE OF deleted_at ON dev_post \
         FOR EACH ROW EXECUTE FUNCTION fail_dev_post_soft_delete()",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert!(matches!(
        controller.delete_post(pin, CREATOR).await,
        Err(crate::result::AppError::InternalError(_))
    ));
    let state: (bool, i64) = sqlx::query_as(
        "SELECT deleted_at IS NULL, \
         (SELECT count(*) FROM dev_post_pin WHERE post_id = $1) \
         FROM dev_post WHERE id = $1",
    )
    .bind(pin)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, (true, 1));
}
