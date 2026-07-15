#[sqlx::test(migrations = "./migrations-test")]
async fn audit_and_title_migrations_have_exact_contract(pool: sqlx::PgPool) {
    let title: (String, String, String) = sqlx::query_as(
        "SELECT data_type, is_nullable, column_default FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='dev_post' AND column_name='title'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(title, ("text".into(), "NO".into(), "''::text".into()));

    let title_indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_index i \
         JOIN pg_class t ON t.oid=i.indrelid \
         JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=ANY(i.indkey) \
         WHERE t.relname='dev_post' AND a.attname='title'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(title_indexes, 0);

    let audit_id_default: Option<String> = sqlx::query_scalar(
        "SELECT column_default FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='dev_post_moderation_log' \
           AND column_name='id'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_id_default, None, "UUID must be application-generated");

    let audit_fks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.table_constraints \
         WHERE table_schema='public' AND table_name='dev_post_moderation_log' \
           AND constraint_type='FOREIGN KEY'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_fks, 0);

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname='public' \
         AND tablename='dev_post_moderation_log' ORDER BY indexname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(indexes.contains(&"idx_dev_post_moderation_log_post_created".into()));
    assert!(indexes.contains(&"idx_dev_post_moderation_log_admin_created".into()));

    for (index, expected) in [
        (
            "idx_dev_post_moderation_log_post_created",
            vec!["post_id".to_string(), "created_at".to_string()],
        ),
        (
            "idx_dev_post_moderation_log_admin_created",
            vec!["admin_account_id".to_string(), "created_at".to_string()],
        ),
    ] {
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT a.attname FROM pg_index i \
             JOIN pg_class idx ON idx.oid=i.indexrelid \
             CROSS JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS key(attnum, ord) \
             JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=key.attnum \
             WHERE idx.relname=$1 ORDER BY key.ord",
        )
        .bind(index)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(columns, expected);
        let definition: String = sqlx::query_scalar(
            "SELECT pg_get_indexdef(indexrelid) FROM pg_index i \
             JOIN pg_class idx ON idx.oid=i.indexrelid WHERE idx.relname=$1",
        )
        .bind(index)
        .fetch_one(&pool)
        .await
        .unwrap();
        let leading_column = if index == "idx_dev_post_moderation_log_post_created" {
            "post_id"
        } else {
            "admin_account_id"
        };
        assert!(
            definition.contains(&format!("({leading_column}, created_at DESC)")),
            "unexpected index direction: {definition}"
        );
    }

    let no_backfill: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(no_backfill, 0);
    let app_uuid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,1,'0x0000000000000000000000000000000000000001', \
                 '0x0000000000000000000000000000000000000002','DELETE',true)",
    )
    .bind(app_uuid)
    .execute(&pool)
    .await
    .unwrap();
    let stored_uuid: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM dev_post_moderation_log WHERE id=$1")
            .bind(app_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_uuid, app_uuid);
    assert!(
        sqlx::query(
            "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,0,'0x1','0x2','DELETE',false)",
        )
        .bind(uuid::Uuid::new_v4())
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,2,'0x1','0x2','EDIT',false)",
        )
        .bind(uuid::Uuid::new_v4())
        .execute(&pool)
        .await
        .is_err()
    );

    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO public.dev_post(token_id,author,title,body) \
         VALUES ('0x0000000000000000000000000000000000000003', \
                 '0x0000000000000000000000000000000000000004','Audit survivor','body') \
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let survivor_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO dev_post_moderation_log \
         (id,post_id,token_id,admin_account_id,action,changed) \
         VALUES ($1,$2,'0x0000000000000000000000000000000000000003', \
                 '0x0000000000000000000000000000000000000004','DELETE',true)",
    )
    .bind(survivor_id)
    .bind(post_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM public.dev_post WHERE id=$1")
        .bind(post_id)
        .execute(&pool)
        .await
        .unwrap();
    let survivor_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dev_post_moderation_log WHERE id=$1")
            .bind(survivor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(survivor_count, 1);
}

#[sqlx::test(migrations = "./migrations-test")]
async fn legacy_split_is_first_lf_crlf_aware_and_relationship_safe(pool: sqlx::PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    sqlx::raw_sql(
        "CREATE TEMP TABLE dev_post (id BIGSERIAL PRIMARY KEY, body TEXT NOT NULL); \
         CREATE TEMP TABLE dev_post_image (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll_option (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_like (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_poll_vote (post_id BIGINT); \
         CREATE TEMP TABLE dev_post_pin (post_id BIGINT);",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
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
            .bind(legacy)
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(id, position as i64 + 1);
    }
    sqlx::raw_sql(
        "INSERT INTO dev_post_image VALUES (1); INSERT INTO dev_post_poll VALUES (1); \
         INSERT INTO dev_post_poll_option VALUES (1); INSERT INTO dev_post_like VALUES (1); \
         INSERT INTO dev_post_poll_vote VALUES (1); INSERT INTO dev_post_pin VALUES (1);",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    let ids_before: Vec<i64> = sqlx::query_scalar("SELECT id FROM dev_post ORDER BY id")
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    let count_before = ids_before.len();
    sqlx::raw_sql(include_str!("../migrations/0041_dev_post_title.sql"))
        .execute(&mut *connection)
        .await
        .unwrap();
    let ids_after: Vec<i64> = sqlx::query_scalar("SELECT id FROM dev_post ORDER BY id")
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert_eq!(ids_after, ids_before);
    assert_eq!(ids_after.len(), count_before);
    let actual: Vec<(String, String)> =
        sqlx::query_as("SELECT title, body FROM dev_post ORDER BY id")
            .fetch_all(&mut *connection)
            .await
            .unwrap();
    let expected: Vec<(String, String)> = cases
        .iter()
        .map(|(_, title, body)| ((*title).into(), (*body).into()))
        .collect();
    assert_eq!(actual, expected);
    let relation_counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM dev_post_image), \
                (SELECT count(*) FROM dev_post_poll), \
                (SELECT count(*) FROM dev_post_poll_option), \
                (SELECT count(*) FROM dev_post_like), \
                (SELECT count(*) FROM dev_post_poll_vote), \
                (SELECT count(*) FROM dev_post_pin)",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(relation_counts, (1, 1, 1, 1, 1, 1));
}
