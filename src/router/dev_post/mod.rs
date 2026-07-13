pub mod handler;
pub mod path;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware as axum_middleware,
    routing::{get, patch, post, put},
};

use path::DevPostPath;

use crate::{middleware::authenticate_user, state::AppState};

/// Public GETs and protected writes share paths (`/dev-post` GET+POST,
/// `/dev-post/{post_id}` GET+PATCH+DELETE), so they're built as two routers
/// and merged — `route_layer` then applies auth only to the protected half.
pub fn router(app_state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route(DevPostPath::Trending.as_str(), get(handler::get_trending))
        .route(DevPostPath::Ranking.as_str(), get(handler::get_ranking))
        .route(DevPostPath::Feed.as_str(), get(handler::get_feed))
        .route(DevPostPath::Detail.as_str(), get(handler::get_detail));

    let protected = Router::new()
        .route(
            DevPostPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)), // 5MB for image upload
        )
        .route(DevPostPath::Create.as_str(), post(handler::create_post))
        .route(
            DevPostPath::Detail.as_str(),
            patch(handler::edit_post).delete(handler::delete_post),
        )
        .route(
            DevPostPath::Like.as_str(),
            post(handler::like).delete(handler::unlike),
        )
        .route(DevPostPath::Vote.as_str(), post(handler::vote))
        .route(
            DevPostPath::Pin.as_str(),
            put(handler::pin_post).delete(handler::unpin_post),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            app_state,
            authenticate_user,
        ));

    public.merge(protected)
}

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

    const CREATOR: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
    const OTHER: &str = "0xde709f2102306220921060314715629080e2fb77";
    const TOKEN: &str = "0x0000000000000000000000000000000000007777";

    async fn test_app(pool: sqlx::PgPool, session_id: &str) -> (Router, Arc<RedisDatabase>) {
        let redis = Arc::new(RedisDatabase::new().await);
        redis
            .set_session(session_id, CREATOR, 60_000)
            .await
            .unwrap();
        let state = AppState {
            postgres: Arc::new(PostgresDatabase {
                write_pool: pool.clone(),
                read_pool: pool,
            }),
            redis: redis.clone(),
            r2: Arc::new(R2Client::new().await),
            capricorn: Arc::new(CapricornClient::new(String::new())),
        };
        let app = router(state.clone())
            .layer(CookieManagerLayer::new())
            .with_state(state);
        (app, redis)
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn actual_router_pin_is_authenticated_empty_204_and_malformed_is_400(pool: sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
             VALUES ($1, 'Token', 'TKN', 'img', $2, 0, '0xhash', 0)",
        )
        .bind(TOKEN)
        .bind(CREATOR)
        .execute(&pool)
        .await
        .unwrap();
        let post_id: i64 = sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'post') RETURNING id",
        )
        .bind(TOKEN)
        .bind(CREATOR)
        .fetch_one(&pool)
        .await
        .unwrap();
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let (app, redis) = test_app(pool, &session_id).await;
        let cookie_name =
            std::env::var("COOKIE_NAME").expect("COOKIE_NAME must be exported before cargo test");
        let cookie = format!("{cookie_name}={session_id}");

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/dev-post/{post_id}/pin"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_json: serde_json::Value = serde_json::from_slice(
            &to_bytes(unauthorized.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(unauthorized_json["error"].is_string());

        let wrong_session = uuid::Uuid::new_v4().simple().to_string();
        redis
            .set_session(&wrong_session, OTHER, 60_000)
            .await
            .unwrap();
        let forbidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/dev-post/{post_id}/pin"))
                    .header(header::COOKIE, format!("{cookie_name}={wrong_session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let forbidden_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(forbidden.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(forbidden_json["error"].is_string());

        let put_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/dev-post/{post_id}/pin"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put_response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            to_bytes(put_response.into_body(), usize::MAX)
                .await
                .unwrap()
                .len(),
            0
        );

        let malformed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/dev-post/not-an-i64/pin")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/dev-post/{post_id}/pin"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            to_bytes(delete_response.into_body(), usize::MAX)
                .await
                .unwrap()
                .len(),
            0
        );

        for method in [Method::GET, Method::POST, Method::PATCH] {
            let unsupported = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri(format!("/dev-post/{post_id}/pin"))
                        .header(header::COOKIE, &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                unsupported.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} must not be registered on the pin route"
            );
        }
        redis.delete_session(&session_id).await.unwrap();
        redis.delete_session(&wrong_session).await.unwrap();
        redis.delete_devpost_feed(TOKEN).await.unwrap();
    }
}
