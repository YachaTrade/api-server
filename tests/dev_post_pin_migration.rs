async fn seed_token(pool: &sqlx::PgPool, token_id: &str, creator: &str) {
    sqlx::query(
        "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
         VALUES ($1, 'Token', 'TKN', 'img', $2, 0, '0xhash', 0)",
    )
    .bind(token_id)
    .bind(creator)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_post(pool: &sqlx::PgPool, token_id: &str, author: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'post') RETURNING id",
    )
    .bind(token_id)
    .bind(author)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations-test")]
async fn dev_post_pin_enforces_uniqueness_foreign_keys_and_cascades(pool: sqlx::PgPool) {
    seed_token(&pool, "0xTokenA", "0xCreator").await;
    seed_token(&pool, "0xTokenB", "0xCreator").await;
    let post_a = seed_post(&pool, "0xTokenA", "0xCreator").await;
    let post_b = seed_post(&pool, "0xTokenB", "0xCreator").await;

    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
        .bind("0xTokenA")
        .bind(post_a)
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
            .bind("0xTokenA")
            .bind(post_b)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2)")
            .bind("0xTokenB")
            .bind(post_a)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ('0xMissing', $1)")
            .bind(post_b)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ('0xTokenB', -1)")
            .execute(&pool)
            .await
            .is_err()
    );

    sqlx::query("DELETE FROM dev_post WHERE id = $1")
        .bind(post_a)
        .execute(&pool)
        .await
        .unwrap();
    let after_post_delete: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE token_id = '0xTokenA'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_post_delete, 0);

    sqlx::query("INSERT INTO dev_post_pin (token_id, post_id) VALUES ('0xTokenB', $1)")
        .bind(post_b)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM token WHERE token_id = '0xTokenB'")
        .execute(&pool)
        .await
        .unwrap();
    let after_token_delete: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
            .bind(post_b)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_token_delete, 0);
}
