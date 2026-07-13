pub enum DevPostPath {
    Trending,
    Ranking,
    UploadImage,
    Feed,
    Create,
    Detail,
    Like,
    Vote,
}

impl DevPostPath {
    /// axum route syntax. axum 0.7 (matchit 0.7) captures with `:param`; `{param}`
    /// is a literal segment there, so it must not be used here — see `docs_str`.
    pub fn as_str(&self) -> &'static str {
        match self {
            DevPostPath::Trending => "/dev-post/trending",
            DevPostPath::Ranking => "/dev-post/ranking",
            DevPostPath::UploadImage => "/dev-post/image",
            DevPostPath::Feed | DevPostPath::Create => "/dev-post",
            DevPostPath::Detail => "/dev-post/:post_id",
            DevPostPath::Like => "/dev-post/:post_id/like",
            DevPostPath::Vote => "/dev-post/:post_id/vote",
        }
    }
    /// OpenAPI path-template syntax for utoipa — `{param}`, not `:param`.
    pub fn docs_str(&self) -> &'static str {
        match self {
            DevPostPath::Trending => "/dev-post/trending",
            DevPostPath::Ranking => "/dev-post/ranking",
            DevPostPath::UploadImage => "/dev-post/image",
            DevPostPath::Feed | DevPostPath::Create => "/dev-post",
            DevPostPath::Detail => "/dev-post/{post_id}",
            DevPostPath::Like => "/dev-post/{post_id}/like",
            DevPostPath::Vote => "/dev-post/{post_id}/vote",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::extract::Path;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    async fn echo(Path(post_id): Path<i64>) -> String {
        post_id.to_string()
    }

    async fn status_for(route: &'static str, uri: &str) -> StatusCode {
        let app: Router<()> = Router::new().route(route, get(echo));
        app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    /// Guards the axum-vs-OpenAPI path-syntax trap: `as_str()` feeds `Router::route`,
    /// which on axum 0.7 treats `{post_id}` as a literal segment and silently 404s
    /// every request. Registers each parameterized route for real and hits it.
    #[tokio::test]
    async fn parameterized_routes_match_real_requests() {
        for (route, uri) in [
            (DevPostPath::Detail.as_str(), "/dev-post/123"),
            (DevPostPath::Like.as_str(), "/dev-post/123/like"),
            (DevPostPath::Vote.as_str(), "/dev-post/123/vote"),
        ] {
            assert_eq!(
                status_for(route, uri).await,
                StatusCode::OK,
                "route `{route}` did not match `{uri}`"
            );
        }
    }

    #[tokio::test]
    async fn static_routes_match_real_requests() {
        for route in [
            DevPostPath::Feed.as_str(),
            DevPostPath::Trending.as_str(),
            DevPostPath::Ranking.as_str(),
            DevPostPath::UploadImage.as_str(),
        ] {
            let app: Router<()> = Router::new().route(route, get(|| async { "ok" }));
            let status = app
                .oneshot(Request::builder().uri(route).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status();
            assert_eq!(
                status,
                StatusCode::OK,
                "route `{route}` did not match itself"
            );
        }
    }

    #[test]
    fn docs_paths_use_openapi_brace_syntax() {
        assert_eq!(DevPostPath::Detail.docs_str(), "/dev-post/{post_id}");
        assert_eq!(DevPostPath::Like.docs_str(), "/dev-post/{post_id}/like");
        assert_eq!(DevPostPath::Vote.docs_str(), "/dev-post/{post_id}/vote");
    }
}
