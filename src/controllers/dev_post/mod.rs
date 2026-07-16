//! Dev Post controller: create/edit/delete (writes) with creator authorization.

pub(crate) mod moderation;
mod pin;
#[cfg(test)]
mod pin_feed_tests;

use crate::db::postgres::PostgresDatabase;
use crate::result::AppError;
use crate::types::dev_post::{
    AuthorSummary, CreateDevPostRequest, DevPostResponse, EditDevPostRequest, FeedBase,
    POLL_DURATION_DAYS, PollOptionResponse, PollResponse, RankingRow, TokenSummary,
    parse_tweet_url,
};
use bigdecimal::BigDecimal;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct DevPostController {
    db: Arc<PostgresDatabase>,
}

impl DevPostController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Single-statement CTE: `pgbouncer` runs in statement pooling mode, which
    /// forbids `BEGIN`/`COMMIT` — a single statement is its own implicit
    /// transaction, so this stays atomic without one. `tok` carries the
    /// creator authorization check, and `new_post` only fires when
    /// `tok.creator = $2` matches; the outer `SELECT` always returns exactly
    /// one row (scalar subqueries against possibly-empty CTEs), which is how
    /// NotFound/Forbidden are distinguished afterwards.
    pub async fn create_post(
        &self,
        author: &str,
        req: &CreateDevPostRequest,
    ) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();

        let poll_labels: Option<Vec<String>> = req
            .poll
            .as_ref()
            .map(|poll| poll.options.iter().map(|o| o.label.clone()).collect());
        let poll_images: Option<Vec<Option<String>>> = req
            .poll
            .as_ref()
            .map(|poll| poll.options.iter().map(|o| o.image_uri.clone()).collect());

        let row: (Option<String>, Option<i64>) = sqlx::query_as(
            "WITH tok AS (
                 SELECT creator FROM token WHERE token_id = $1
             ),
             new_post AS (
                 INSERT INTO dev_post (token_id, author, title, body)
                 SELECT $1, $2, $3, $4 FROM tok WHERE tok.creator = $2
                 RETURNING id, created_at
             ),
             img AS (
                 INSERT INTO dev_post_image (post_id, position, image_uri)
                 SELECT np.id, (u.ord - 1)::smallint, u.uri
                 FROM new_post np, unnest($5::text[]) WITH ORDINALITY AS u(uri, ord)
             ),
             poll AS (
                 INSERT INTO dev_post_poll (post_id, closes_at)
                 SELECT np.id, np.created_at + ($6 || ' days')::interval
                 FROM new_post np
                 WHERE $7::text[] IS NOT NULL
                 RETURNING post_id
             ),
             opt AS (
                 INSERT INTO dev_post_poll_option (post_id, position, label, image_uri)
                 SELECT pl.post_id, u.ord::smallint, u.label, u.image_uri
                 FROM poll pl, unnest($7::text[], $8::text[]) WITH ORDINALITY AS u(label, image_uri, ord)
             )
             SELECT (SELECT creator FROM tok) AS creator, (SELECT id FROM new_post) AS id",
        )
        .bind(&req.token_id)
        .bind(author)
        .bind(&req.title)
        .bind(req.body.as_deref().unwrap_or(""))
        .bind(&req.image_uris)
        .bind(POLL_DURATION_DAYS.to_string())
        .bind(&poll_labels)
        .bind(&poll_images)
        .fetch_one(pool)
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))?;

        let (creator, id) = row;
        match creator {
            None => return Err(AppError::NotFound("Token not found".into())),
            Some(c) if c != author => {
                return Err(AppError::Forbidden("Only the coin creator can post".into()));
            }
            _ => {}
        }
        id.ok_or_else(|| AppError::InternalError("dev post insert did not return id".into()))
    }

    /// Single-statement CTE. `target` carries the author authorization check
    /// (`deleted_at IS NULL`); `upd` and the image CTEs all gate on
    /// `EXISTS (SELECT 1 FROM target WHERE target.author = $2)` so a
    /// non-author request touches nothing. Images use upsert + disjoint
    /// tail-delete rather than delete-then-insert: two data-modifying CTEs
    /// deleting and inserting the same `(post_id, position)` key in one
    /// statement hit `duplicate key value violates unique constraint`
    /// because CTE execution order is unspecified.
    pub async fn edit_post(
        &self,
        post_id: i64,
        author: &str,
        req: &EditDevPostRequest,
    ) -> Result<String, AppError> {
        let pool = self.db.get_write_pool();

        let row: (Option<String>, Option<String>) = sqlx::query_as(
            "WITH target AS (
                 SELECT id, author FROM dev_post WHERE id = $1 AND deleted_at IS NULL
             ),
             upd AS (
                 UPDATE dev_post
                 SET title = COALESCE($3, title), body = COALESCE($4, body),
                     updated_at = NOW(), edited_at = NOW()
                 WHERE id = $1 AND EXISTS (SELECT 1 FROM target WHERE target.author = $2)
                 RETURNING token_id
             ),
             img_upsert AS (
                 INSERT INTO dev_post_image (post_id, position, image_uri)
                 SELECT $1, (u.ord - 1)::smallint, u.uri
                 FROM target t, unnest($5::text[]) WITH ORDINALITY AS u(uri, ord)
                 WHERE t.author = $2
                 ON CONFLICT (post_id, position) DO UPDATE SET image_uri = EXCLUDED.image_uri
             ),
             img_trim AS (
                 DELETE FROM dev_post_image
                 WHERE post_id = $1
                   AND $5::text[] IS NOT NULL
                   AND position >= cardinality($5::text[])
                   AND EXISTS (SELECT 1 FROM target WHERE target.author = $2)
             )
             SELECT (SELECT author FROM target) AS author, (SELECT token_id FROM upd) AS token_id",
        )
        .bind(post_id)
        .bind(author)
        .bind(req.title.value())
        .bind(req.body.as_deref())
        .bind(&req.image_uris)
        .fetch_one(pool)
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))?;

        let (existing_author, token_id) = row;
        let existing_author =
            existing_author.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        if existing_author != author {
            return Err(AppError::Forbidden("Not the post author".into()));
        }
        token_id.ok_or_else(|| {
            AppError::InternalError("dev post update did not return token_id".into())
        })
    }

    pub async fn delete_post(
        &self,
        post_id: i64,
        author: &str,
    ) -> Result<moderation::CommitOutcome<moderation::PostMutationContext>, AppError> {
        self.delete_post_as_author(post_id, author).await
    }

    pub async fn like(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();
        // guard: post exists & not deleted (FK would allow liking a deleted post otherwise)
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM dev_post WHERE id=$1 AND deleted_at IS NULL")
                .bind(post_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        if exists.is_none() {
            return Err(AppError::NotFound("Post not found".into()));
        }
        sqlx::query(
            "INSERT INTO dev_post_like (post_id, account_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(post_id)
        .bind(account_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        Self::count_likes(pool, post_id).await
    }

    pub async fn unlike(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();
        sqlx::query("DELETE FROM dev_post_like WHERE post_id=$1 AND account_id=$2")
            .bind(post_id)
            .bind(account_id)
            .execute(pool)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        Self::count_likes(pool, post_id).await
    }

    pub async fn like_count(&self, post_id: i64) -> Result<i64, AppError> {
        Self::count_likes(self.db.get_read_pool(), post_id).await
    }

    async fn count_likes<'e, E>(exec: E, post_id: i64) -> Result<i64, AppError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query_scalar("SELECT count(*) FROM dev_post_like WHERE post_id=$1")
            .bind(post_id)
            .fetch_one(exec)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))
    }

    /// Single-statement CTE. The closed-poll check moves from Rust's
    /// `Utc::now()` to SQL's `NOW()` — a single clock, constant within the
    /// statement, instead of two independent clocks racing each other.
    pub async fn vote(
        &self,
        post_id: i64,
        account_id: &str,
        option_position: i16,
    ) -> Result<(), AppError> {
        let pool = self.db.get_write_pool();

        let row: (Option<bool>, Option<bool>, Option<bool>) = sqlx::query_as(
            "WITH poll AS (
                 SELECT p.closes_at FROM dev_post_poll p
                 JOIN dev_post d ON d.id = p.post_id
                 WHERE p.post_id = $1 AND d.deleted_at IS NULL
             ),
             opt AS (
                 SELECT position FROM dev_post_poll_option WHERE post_id = $1 AND position = $3
             ),
             v AS (
                 INSERT INTO dev_post_poll_vote (post_id, account_id, option_position)
                 SELECT $1, $2, $3 FROM poll, opt WHERE poll.closes_at > NOW()
                 ON CONFLICT (post_id, account_id)
                 DO UPDATE SET option_position = EXCLUDED.option_position, updated_at = NOW()
             )
             SELECT (SELECT true FROM poll) AS poll_exists,
                    (SELECT closes_at > NOW() FROM poll) AS poll_open,
                    (SELECT true FROM opt) AS option_exists",
        )
        .bind(post_id)
        .bind(account_id)
        .bind(option_position)
        .fetch_one(pool)
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))?;

        let (poll_exists, poll_open, option_exists) = row;
        if poll_exists != Some(true) {
            return Err(AppError::NotFound("Poll not found".into()));
        }
        if poll_open != Some(true) {
            return Err(AppError::Conflict("Poll is closed".into()));
        }
        if option_exists != Some(true) {
            return Err(AppError::BadRequest("Invalid option_position".into()));
        }
        Ok(())
    }

    // ---- reads ----

    /// Batch-loads images, like counts, and poll+options+vote-counts for a page
    /// of `dev_post` ids, then assembles `DevPostResponse`s preserving the given
    /// id order. Batches via `= ANY($1)` instead of a query per post to avoid
    /// N+1. Viewer-agnostic: `liked_by_me` is always `false` and poll
    /// `my_vote_option` is always `None` — callers needing personalization
    /// layer it on with `apply_personalization`. This split lets the base
    /// response be cached and shared across viewers.
    async fn hydrate_base(
        &self,
        pool: &sqlx::PgPool,
        ids: &[i64],
    ) -> Result<Vec<DevPostResponse>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(sqlx::FromRow)]
        struct CoreRow {
            id: i64,
            token_id: String,
            author: String,
            title: String,
            body: String,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
            edited_at: Option<chrono::DateTime<chrono::Utc>>,
            token_name: String,
            token_symbol: String,
            token_image: String,
            author_nickname: Option<String>,
            author_image: Option<String>,
            market_cap: Option<BigDecimal>,
        }
        let core_rows: Vec<CoreRow> = sqlx::query_as(
            "SELECT dp.id, dp.token_id, dp.author, dp.title, dp.body, dp.created_at, dp.updated_at, dp.edited_at,
                    t.name AS token_name, t.symbol AS token_symbol, t.image_uri AS token_image,
                    a.nickname AS author_nickname, a.image_uri AS author_image,
                    (m.price * t.total_supply * COALESCE(lp.price, 0)) AS market_cap
             FROM dev_post dp
             JOIN token t ON t.token_id = dp.token_id
             LEFT JOIN account a ON a.account_id = dp.author
             LEFT JOIN market m ON m.token_id = dp.token_id
             LEFT JOIN LATERAL (
                 SELECT p.price FROM price p
                 WHERE p.quote_id = m.quote_id
                 ORDER BY p.block_number DESC
                 LIMIT 1
             ) lp ON true
             WHERE dp.id = ANY($1) AND dp.deleted_at IS NULL",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        let mut core_by_id: HashMap<i64, CoreRow> =
            core_rows.into_iter().map(|r| (r.id, r)).collect();

        #[derive(sqlx::FromRow)]
        struct ImageRow {
            post_id: i64,
            image_uri: String,
        }
        let image_rows: Vec<ImageRow> = sqlx::query_as(
            "SELECT post_id, image_uri FROM dev_post_image WHERE post_id = ANY($1) ORDER BY post_id, position",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        let mut images_by_post: HashMap<i64, Vec<String>> = HashMap::new();
        for row in image_rows {
            images_by_post
                .entry(row.post_id)
                .or_default()
                .push(row.image_uri);
        }

        let like_count_rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT post_id, count(*) FROM dev_post_like WHERE post_id = ANY($1) GROUP BY post_id",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        let like_counts: HashMap<i64, i64> = like_count_rows.into_iter().collect();

        let poll_rows: Vec<(i64, chrono::DateTime<chrono::Utc>)> =
            sqlx::query_as("SELECT post_id, closes_at FROM dev_post_poll WHERE post_id = ANY($1)")
                .bind(ids)
                .fetch_all(pool)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        let poll_closes: HashMap<i64, chrono::DateTime<chrono::Utc>> =
            poll_rows.into_iter().collect();

        #[derive(sqlx::FromRow)]
        struct OptionRow {
            post_id: i64,
            position: i16,
            label: String,
            image_uri: Option<String>,
            votes: i64,
        }
        let option_rows: Vec<OptionRow> = sqlx::query_as(
            "SELECT o.post_id, o.position, o.label, o.image_uri, COUNT(v.account_id) AS votes
             FROM dev_post_poll_option o
             LEFT JOIN dev_post_poll_vote v ON v.post_id = o.post_id AND v.option_position = o.position
             WHERE o.post_id = ANY($1) GROUP BY o.post_id, o.position, o.label, o.image_uri ORDER BY o.post_id, o.position",
        )
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        let mut options_by_post: HashMap<i64, Vec<PollOptionResponse>> = HashMap::new();
        for row in option_rows {
            options_by_post
                .entry(row.post_id)
                .or_default()
                .push(PollOptionResponse {
                    position: row.position,
                    label: row.label,
                    image_uri: row.image_uri,
                    vote_count: row.votes,
                });
        }

        let now = chrono::Utc::now();
        let mut posts = Vec::with_capacity(ids.len());
        for &id in ids {
            // ids are pre-filtered by each caller's query; a missing core row here
            // would mean the post vanished between queries, so just skip it.
            let Some(core) = core_by_id.remove(&id) else {
                continue;
            };
            let poll = poll_closes.get(&id).map(|closes_at| {
                let options = options_by_post.remove(&id).unwrap_or_default();
                let total_votes = options.iter().map(|o| o.vote_count).sum();
                PollResponse {
                    closes_at: *closes_at,
                    is_closed: *closes_at <= now,
                    total_votes,
                    options,
                    my_vote_option: None,
                }
            });
            posts.push(DevPostResponse {
                id: id.to_string(),
                token: TokenSummary {
                    token_id: core.token_id,
                    name: core.token_name,
                    symbol: core.token_symbol,
                    image_uri: Some(core.token_image),
                    market_cap: core.market_cap.map(|b| b.normalized().to_plain_string()),
                },
                author: AuthorSummary {
                    account_id: core.author,
                    nickname: core.author_nickname,
                    image_uri: core.author_image,
                },
                title: core.title,
                tweet_url: parse_tweet_url(&core.body),
                body: core.body,
                images: images_by_post.remove(&id).unwrap_or_default(),
                poll,
                like_count: *like_counts.get(&id).unwrap_or(&0),
                liked_by_me: false,
                is_edited: core.edited_at.is_some(),
                created_at: core.created_at,
                updated_at: core.updated_at,
            });
        }
        Ok(posts)
    }

    /// Overlays viewer-specific personalization (`liked_by_me`, poll
    /// `my_vote_option`) onto already-hydrated base responses, in place, on
    /// the given pool.
    async fn apply_personalization_on(
        &self,
        pool: &sqlx::Pool<sqlx::Postgres>,
        posts: &mut [DevPostResponse],
        viewer: &str,
    ) -> Result<(), AppError> {
        if posts.is_empty() {
            return Ok(());
        }
        let ids: Vec<i64> = posts
            .iter()
            .map(|p| {
                p.id.parse()
                    .expect("DevPostResponse.id is always a valid i64")
            })
            .collect();

        let liked_by_me: HashSet<i64> = {
            let rows: Vec<i64> = sqlx::query_scalar(
                "SELECT post_id FROM dev_post_like WHERE post_id = ANY($1) AND account_id = $2",
            )
            .bind(&ids)
            .bind(viewer)
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
            rows.into_iter().collect()
        };

        let my_votes: HashMap<i64, i16> = {
            let rows: Vec<(i64, i16)> = sqlx::query_as(
                "SELECT post_id, option_position FROM dev_post_poll_vote WHERE post_id = ANY($1) AND account_id = $2",
            )
            .bind(&ids)
            .bind(viewer)
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
            rows.into_iter().collect()
        };

        for post in posts.iter_mut() {
            let id: i64 = post
                .id
                .parse()
                .expect("DevPostResponse.id is always a valid i64");
            post.liked_by_me = liked_by_me.contains(&id);
            if let Some(poll) = post.poll.as_mut() {
                poll.my_vote_option = my_votes.get(&id).copied();
            }
        }
        Ok(())
    }

    /// Overlays viewer-specific personalization (`liked_by_me`, poll
    /// `my_vote_option`) onto already-hydrated base responses, in place.
    /// Always reads `self.db.get_read_pool()` internally so callers — notably
    /// the cache layer applying this to a cached base — don't need to thread
    /// a pool handle through.
    pub async fn apply_personalization(
        &self,
        posts: &mut [DevPostResponse],
        viewer: &str,
    ) -> Result<(), AppError> {
        self.apply_personalization_on(self.db.get_read_pool(), posts, viewer)
            .await
    }

    pub async fn apply_feed_personalization(
        &self,
        feed: &mut FeedBase,
        viewer: &str,
    ) -> Result<(), AppError> {
        let pin_id = feed.pin.as_ref().map(|post| post.id.clone());
        let mut combined = Vec::with_capacity(feed.posts.len() + usize::from(feed.pin.is_some()));
        if let Some(pin) = feed.pin.take() {
            combined.push(pin);
        }
        combined.append(&mut feed.posts);
        self.apply_personalization(&mut combined, viewer).await?;

        feed.pin = pin_id.and_then(|id| {
            combined
                .iter()
                .position(|post| post.id == id)
                .map(|index| combined.remove(index))
        });
        feed.posts = combined;
        Ok(())
    }

    /// `hydrate_base` plus, when a viewer is present, personalization on the
    /// SAME pool `hydrate_base` used. This matters for the read-after-write
    /// path (`get_post_on` → `get_post_rw`): a vote just written on the write
    /// pool must be read back from that same pool, not the (possibly
    /// lagging) read replica — otherwise a vote can commit and then
    /// immediately read back as `my_vote_option: null`.
    async fn hydrate(
        &self,
        pool: &sqlx::PgPool,
        ids: &[i64],
        viewer: Option<&str>,
    ) -> Result<Vec<DevPostResponse>, AppError> {
        let mut posts = self.hydrate_base(pool, ids).await?;
        if let Some(viewer) = viewer {
            self.apply_personalization_on(pool, &mut posts, viewer)
                .await?;
        }
        Ok(posts)
    }

    /// Viewer-agnostic feed page — cacheable. See `get_feed` for the
    /// personalized variant.
    pub async fn get_feed_base(
        &self,
        token_id: Option<&str>,
        page: i64,
        limit: i64,
    ) -> Result<FeedBase, AppError> {
        let pool = self.db.get_read_pool();
        let offset = (page - 1) * limit;
        if let Some(token_id) = token_id {
            let selection = pin::select_token_feed(pool, token_id, limit, offset).await?;
            let emitted_pin_id = (page == 1).then_some(selection.active_pin_id).flatten();
            let mut ids = Vec::with_capacity(
                selection.page_ids.len() + usize::from(emitted_pin_id.is_some()),
            );
            ids.extend(emitted_pin_id);
            ids.extend(selection.page_ids.iter().copied());
            let mut hydrated = self.hydrate_base(pool, &ids).await?;
            let pin = emitted_pin_id.and_then(|pin_id| {
                let id = pin_id.to_string();
                hydrated
                    .iter()
                    .position(|post| post.id == id)
                    .map(|index| hydrated.remove(index))
            });
            return Ok(FeedBase {
                pin,
                posts: hydrated,
                total_count: selection.total_count,
            });
        }

        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM dev_post WHERE deleted_at IS NULL \
             ORDER BY id DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        let total_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dev_post WHERE deleted_at IS NULL")
                .fetch_one(pool)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(FeedBase {
            pin: None,
            posts: self.hydrate_base(pool, &ids).await?,
            total_count,
        })
    }

    pub async fn get_feed(
        &self,
        token_id: Option<&str>,
        page: i64,
        limit: i64,
        viewer: Option<&str>,
    ) -> Result<FeedBase, AppError> {
        let mut feed = self.get_feed_base(token_id, page, limit).await?;
        if let Some(viewer) = viewer {
            self.apply_feed_personalization(&mut feed, viewer).await?;
        }
        Ok(feed)
    }

    /// Shared hydrate-a-single-post path, parameterized on the pool so callers
    /// can choose read-after-write consistency (see `get_post_rw`).
    async fn get_post_on(
        &self,
        pool: &sqlx::PgPool,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM dev_post WHERE id = $1 AND deleted_at IS NULL")
                .bind(post_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        if exists.is_none() {
            return Err(AppError::NotFound("Post not found".into()));
        }
        self.hydrate(pool, &[post_id], viewer)
            .await?
            .pop()
            .ok_or_else(|| AppError::NotFound("Post not found".into()))
    }

    /// Viewer-agnostic single-post fetch — cacheable. See `get_post` for the
    /// personalized variant.
    pub async fn get_post_base(&self, post_id: i64) -> Result<DevPostResponse, AppError> {
        let pool = self.db.get_read_pool();
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM dev_post WHERE id = $1 AND deleted_at IS NULL")
                .bind(post_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        if exists.is_none() {
            return Err(AppError::NotFound("Post not found".into()));
        }
        self.hydrate_base(pool, &[post_id])
            .await?
            .pop()
            .ok_or_else(|| AppError::NotFound("Post not found".into()))
    }

    pub async fn get_post(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        let mut post = self.get_post_base(post_id).await?;
        if let Some(viewer) = viewer {
            self.apply_personalization(std::slice::from_mut(&mut post), viewer)
                .await?;
        }
        Ok(post)
    }

    /// Reads the just-written post back from the write pool instead of the
    /// (possibly lagging) replica — for create/edit/vote response hydration,
    /// where a replica read can 404 a request that just committed.
    pub async fn get_post_rw(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        self.get_post_on(self.db.get_write_pool(), post_id, viewer)
            .await
    }

    /// Viewer-agnostic trending posts (top 3 by 7-day likes) — cacheable. See
    /// `get_trending` for the personalized variant.
    pub async fn get_trending_base(&self) -> Result<Vec<DevPostResponse>, AppError> {
        let pool = self.db.get_read_pool();
        // LEFT JOIN (not INNER) so posts with zero likes still get a row with
        // count 0 — they sort after liked posts but keep the section from
        // going empty before any likes have accrued. `count(l.account_id)`,
        // not `count(*)`: the latter counts the unmatched LEFT JOIN row itself.
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT p.id FROM dev_post p
             LEFT JOIN dev_post_like l ON l.post_id = p.id AND l.created_at >= NOW() - INTERVAL '7 days'
             WHERE p.deleted_at IS NULL
             GROUP BY p.id, p.created_at
             ORDER BY count(l.account_id) DESC, p.created_at DESC, p.id DESC LIMIT 3",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
        self.hydrate_base(pool, &ids).await
    }

    pub async fn get_trending(
        &self,
        viewer: Option<&str>,
    ) -> Result<Vec<DevPostResponse>, AppError> {
        let mut posts = self.get_trending_base().await?;
        if let Some(viewer) = viewer {
            self.apply_personalization(&mut posts, viewer).await?;
        }
        Ok(posts)
    }

    pub async fn get_ranking(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<(Vec<RankingRow>, i64), AppError> {
        let pool = self.db.get_read_pool();
        let offset = (page - 1) * limit;

        // token joined directly here (rather than a second batch hydrate step) since
        // ranking rows are per-coin already, so it's a single indexed query either way.
        //
        // ORDER BY ends on token_id: neither total_likes nor last_posted_at is unique,
        // and OFFSET paging over a non-strict order lets tied coins repeat on one page
        // and vanish from another.
        #[derive(sqlx::FromRow)]
        struct RankRow {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image: String,
            total_likes: i64,
            post_count: i64,
            last_posted_at: Option<chrono::DateTime<chrono::Utc>>,
            market_cap: Option<BigDecimal>,
        }
        let rows: Vec<RankRow> = sqlx::query_as(
            "SELECT p.token_id, t.name AS token_name, t.symbol AS token_symbol, t.image_uri AS token_image,
                    COUNT(l.account_id) AS total_likes,
                    COUNT(DISTINCT p.id) AS post_count,
                    MAX(p.created_at) AS last_posted_at,
                    (m.price * t.total_supply * COALESCE(lp.price, 0) / 1e18) AS market_cap
             FROM dev_post p
             JOIN token t ON t.token_id = p.token_id
             LEFT JOIN dev_post_like l ON l.post_id = p.id
             LEFT JOIN market m ON m.token_id = p.token_id
             LEFT JOIN LATERAL (
                 SELECT pr.price FROM price pr
                 WHERE pr.quote_id = m.quote_id
                 ORDER BY pr.block_number DESC
                 LIMIT 1
             ) lp ON true
             WHERE p.deleted_at IS NULL
             GROUP BY p.token_id, t.name, t.symbol, t.image_uri, m.price, t.total_supply, lp.price
             ORDER BY total_likes DESC, last_posted_at DESC, p.token_id ASC
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT token_id) FROM dev_post WHERE deleted_at IS NULL",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        let rankings = rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| RankingRow {
                rank: offset + i as i64 + 1,
                token: TokenSummary {
                    token_id: r.token_id,
                    name: r.token_name,
                    symbol: r.token_symbol,
                    image_uri: Some(r.token_image),
                    market_cap: r.market_cap.map(|b| b.normalized().to_plain_string()),
                },
                total_likes: r.total_likes,
                post_count: r.post_count,
                last_posted_at: r.last_posted_at,
            })
            .collect();

        Ok((rankings, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::dev_post::*;
    use std::sync::Arc;

    const TOKEN: &str = "0x0000000000000000000000000000000000007777";
    const CREATOR: &str = "0x52908400098527886E0F7030069857D2E4169EE7";

    async fn seed_token(pool: &sqlx::PgPool, token_id: &str, creator: &str) {
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply)
             VALUES ($1,'T','TKN','img',$2,0,'0xhash',0) ON CONFLICT (token_id) DO NOTHING",
        )
        .bind(token_id)
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_total_supply(pool: &sqlx::PgPool, token_id: &str, total_supply: &str) {
        sqlx::query("UPDATE token SET total_supply = $2::numeric WHERE token_id = $1")
            .bind(token_id)
            .bind(total_supply)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_market(pool: &sqlx::PgPool, token_id: &str, price: &str) {
        sqlx::query(
            "INSERT INTO market (market_type, token_id, price, quote_id, latest_trade_at, created_at)
             VALUES ('CURVE', $1, $2::numeric, '0xQuote', 0, 0) ON CONFLICT (token_id) DO NOTHING",
        )
        .bind(token_id)
        .bind(price)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_quote_usd_price(pool: &sqlx::PgPool, usd: &str) {
        sqlx::query(
            "INSERT INTO price (quote_id, block_number, price) VALUES ('0xQuote', 1, $1::numeric)
             ON CONFLICT DO NOTHING",
        )
        .bind(usd)
        .execute(pool)
        .await
        .unwrap();
    }

    fn ctl(pool: sqlx::PgPool) -> DevPostController {
        DevPostController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    fn controller(pool: sqlx::PgPool) -> DevPostController {
        ctl(pool)
    }

    async fn seed_titled_post(pool: &sqlx::PgPool, title: &str, body: &str) -> i64 {
        seed_token(pool, TOKEN, CREATOR).await;
        sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, title, body) \
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(TOKEN)
        .bind(CREATOR)
        .bind(title)
        .bind(body)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn seed_titled_post_with_image(
        pool: &sqlx::PgPool,
        title: &str,
        body: &str,
        image_uri: &str,
    ) -> i64 {
        let id = seed_titled_post(pool, title, body).await;
        sqlx::query(
            "INSERT INTO dev_post_image (post_id, position, image_uri) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(0_i16)
        .bind(image_uri)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_stores_title_and_empty_description_separately(pool: sqlx::PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        let controller = controller(pool.clone());
        let request: CreateDevPostRequest = serde_json::from_str(
            r#"{"token_id":"0x0000000000000000000000000000000000007777","title":"  Multi\nline  "}"#,
        )
        .unwrap();
        let id = controller.create_post(CREATOR, &request).await.unwrap();
        let stored: (String, String) =
            sqlx::query_as("SELECT title, body FROM dev_post WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, ("  Multi\nline  ".into(), "".into()));
        let response = controller.get_post_rw(id, Some(CREATOR)).await.unwrap();
        assert_eq!(response.title, "  Multi\nline  ");
        assert_eq!(response.body, "");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_title_tristate_is_atomic_with_body_and_images(pool: sqlx::PgPool) {
        let id = seed_titled_post(&pool, "Original", "description").await;
        let controller = controller(pool.clone());
        let omitted: EditDevPostRequest = serde_json::from_str(r#"{"body":"new body"}"#).unwrap();
        controller.edit_post(id, CREATOR, &omitted).await.unwrap();
        let after_omitted: (String, String) =
            sqlx::query_as("SELECT title, body FROM dev_post WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after_omitted, ("Original".into(), "new body".into()));

        let title_only: EditDevPostRequest =
            serde_json::from_str(r#"{"title":"  Replaced\nexactly  "}"#).unwrap();
        controller
            .edit_post(id, CREATOR, &title_only)
            .await
            .unwrap();
        let response = controller.get_post_rw(id, Some(CREATOR)).await.unwrap();
        assert_eq!(response.title, "  Replaced\nexactly  ");
        assert_eq!(response.body, "new body");
        assert!(response.is_edited);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn every_read_surface_returns_title_and_description_only_body(pool: sqlx::PgPool) {
        let id = seed_titled_post(
            &pool,
            "Headline",
            "https://x.com/naddotfun/status/1 details",
        )
        .await;
        let controller = controller(pool.clone());
        controller.pin_post(id, CREATOR).await.unwrap();
        let feed = controller.get_feed_base(Some(TOKEN), 1, 20).await.unwrap();
        assert_eq!(feed.pin.as_ref().unwrap().title, "Headline");
        assert_eq!(
            feed.pin.as_ref().unwrap().body,
            "https://x.com/naddotfun/status/1 details"
        );
        let detail = controller.get_post_rw(id, None).await.unwrap();
        assert_eq!(detail.title, "Headline");
        assert_eq!(
            detail.tweet_url.as_deref(),
            Some("https://x.com/naddotfun/status/1")
        );
        let trending = controller.get_trending_base().await.unwrap();
        assert!(
            trending
                .iter()
                .any(|post| post.id == id.to_string() && post.title == "Headline")
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_title_body_and_images_roll_back_atomically(pool: sqlx::PgPool) {
        let id = seed_titled_post_with_image(&pool, "Original", "description", "old-image").await;
        sqlx::raw_sql(
            "CREATE FUNCTION fail_new_image() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'injected image failure'; END $$; \
             CREATE TRIGGER fail_new_image BEFORE INSERT ON dev_post_image \
             FOR EACH ROW EXECUTE FUNCTION fail_new_image()",
        )
        .execute(&pool)
        .await
        .unwrap();
        let request: EditDevPostRequest = serde_json::from_str(
            r#"{"title":"Replacement","body":"replacement body","image_uris":["new-image"]}"#,
        )
        .unwrap();
        assert!(matches!(
            controller(pool.clone())
                .edit_post(id, CREATOR, &request)
                .await,
            Err(AppError::InternalError(_))
        ));
        let stored: (String, String, String) = sqlx::query_as(
            "SELECT dp.title,dp.body,dpi.image_uri FROM dev_post dp \
             JOIN dev_post_image dpi ON dpi.post_id=dp.id WHERE dp.id=$1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            ("Original".into(), "description".into(), "old-image".into())
        );
    }

    async fn create_poll_post(c: &DevPostController) -> i64 {
        c.create_post(
            "0xCreator",
            &CreateDevPostRequest {
                token_id: "0xToken".into(),
                title: "Announcement".into(),
                body: None,
                image_uris: None,
                poll: Some(CreatePollRequest {
                    options: vec![
                        CreatePollOptionRequest {
                            label: "A".into(),
                            image_uri: None,
                        },
                        CreatePollOptionRequest {
                            label: "B".into(),
                            image_uri: None,
                        },
                    ],
                }),
            },
        )
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_rejects_non_creator(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let req = CreateDevPostRequest {
            token_id: "0xToken".into(),
            title: "Announcement".into(),
            body: Some("hi".into()),
            image_uris: None,
            poll: None,
        };
        let err = c.create_post("0xNotCreator", &req).await.unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_rejects_missing_token(pool: sqlx::PgPool) {
        let c = ctl(pool);
        let req = CreateDevPostRequest {
            token_id: "0xMissing".into(),
            title: "Announcement".into(),
            body: Some("hi".into()),
            image_uris: None,
            poll: None,
        };
        let err = c.create_post("0xWhoever", &req).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_with_images_and_poll(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let req = CreateDevPostRequest {
            token_id: "0xToken".into(),
            title: "Announcement".into(),
            body: Some("gm https://x.com/a/status/1".into()),
            image_uris: Some(vec!["u1".into(), "u2".into()]),
            poll: Some(CreatePollRequest {
                options: vec![
                    CreatePollOptionRequest {
                        label: "A".into(),
                        image_uri: None,
                    },
                    CreatePollOptionRequest {
                        label: "B".into(),
                        image_uri: Some("ib".into()),
                    },
                ],
            }),
        };
        let id = c.create_post("0xCreator", &req).await.unwrap();
        let imgs: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_image WHERE post_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let opts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dev_post_poll_option WHERE post_id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let closes: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT p.created_at, poll.closes_at FROM dev_post p JOIN dev_post_poll poll ON poll.post_id=p.id WHERE p.id=$1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(imgs, 2);
        assert_eq!(opts, 2);
        assert_eq!((closes.1 - closes.0).num_days(), 14);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_by_non_author_forbidden(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let err = c
            .edit_post(
                id,
                "0xSomeoneElse",
                &EditDevPostRequest {
                    title: Default::default(),
                    body: Some("y".into()),
                    image_uris: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_updates_body_and_replaces_images(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: Some(vec!["old".into()]),
                    poll: None,
                },
            )
            .await
            .unwrap();
        c.edit_post(
            id,
            "0xCreator",
            &EditDevPostRequest {
                title: Default::default(),
                body: Some("new body".into()),
                image_uris: Some(vec!["new1".into(), "new2".into()]),
            },
        )
        .await
        .unwrap();
        let (body,): (String,) = sqlx::query_as("SELECT body FROM dev_post WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(body, "new body");
        let imgs: Vec<String> = sqlx::query_scalar(
            "SELECT image_uri FROM dev_post_image WHERE post_id=$1 ORDER BY position",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(imgs, vec!["new1".to_string(), "new2".to_string()]);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_missing_post_not_found(pool: sqlx::PgPool) {
        let c = ctl(pool);
        let err = c
            .edit_post(
                999,
                "0xCreator",
                &EditDevPostRequest {
                    title: Default::default(),
                    body: Some("y".into()),
                    image_uris: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delete_soft_hides_post(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        c.delete_post(id, "0xCreator").await.unwrap();
        let deleted: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM dev_post WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(deleted.is_some());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delete_by_non_author_forbidden(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let err = c.delete_post(id, "0xSomeoneElse").await.unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delete_missing_post_not_found(pool: sqlx::PgPool) {
        let c = ctl(pool);
        let err = c.delete_post(999, "0xCreator").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn like_is_idempotent_toggle(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(c.like(id, "0xUser").await.unwrap(), 1);
        assert_eq!(c.like(id, "0xUser").await.unwrap(), 1); // idempotent, still 1
        assert_eq!(c.like(id, "0xUser2").await.unwrap(), 2);
        assert_eq!(c.unlike(id, "0xUser").await.unwrap(), 1);
        assert_eq!(c.unlike(id, "0xUser").await.unwrap(), 1); // idempotent
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn like_deleted_post_404(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        c.delete_post(id, "0xCreator").await.unwrap();
        assert!(matches!(
            c.like(id, "0xUser").await.unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn vote_then_change(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        c.vote(id, "0xUser", 1).await.unwrap();
        c.vote(id, "0xUser", 2).await.unwrap(); // change
        let (opt, n): (i16, i64) = sqlx::query_as(
            "SELECT option_position, count(*) OVER () FROM dev_post_poll_vote WHERE post_id=$1 AND account_id=$2",
        )
        .bind(id)
        .bind("0xUser")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(opt, 2);
        assert_eq!(n, 1); // still exactly one vote row
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn vote_bad_option_400(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        assert!(matches!(
            c.vote(id, "0xUser", 9).await.unwrap_err(),
            AppError::BadRequest(_)
        ));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn vote_closed_poll_409(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        sqlx::query(
            "UPDATE dev_post_poll SET closes_at = NOW() - INTERVAL '1 day' WHERE post_id=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            c.vote(id, "0xUser", 1).await.unwrap_err(),
            AppError::Conflict(_)
        ));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn feed_excludes_deleted_and_orders_newest_first(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let a = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("a".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let b = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("b".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        c.delete_post(a, "0xCreator").await.unwrap();
        let feed = c.get_feed(Some("0xToken"), 1, 10, None).await.unwrap();
        assert_eq!(feed.total_count, 1);
        assert_eq!(feed.posts[0].id, b.to_string());
        assert!(!feed.posts[0].liked_by_me);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_post_rw_hydrates_from_write_pool(pool: sqlx::PgPool) {
        // Single test pool can't simulate replica lag, so this only proves
        // get_post_rw hydrates correctly on the write pool. The read-after-write
        // guarantee itself comes from get_post_on/hydrate personalizing on the
        // same pool arg they base-hydrate on, so get_post_rw's write-pool choice
        // flows through to apply_personalization_on too (not just hydrate_base).
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("gm from write pool".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let r = c.get_post_rw(id, None).await.unwrap();
        assert_eq!(r.id, id.to_string());
        assert_eq!(r.body, "gm from write pool");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn detail_personalization_and_poll(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        c.like(id, "0xU").await.unwrap();
        c.vote(id, "0xU", 2).await.unwrap();
        let r = c.get_post(id, Some("0xU")).await.unwrap();
        assert!(r.liked_by_me);
        assert_eq!(r.like_count, 1);
        let poll = r.poll.unwrap();
        assert_eq!(poll.my_vote_option, Some(2));
        assert_eq!(poll.total_votes, 1);
        assert!(!poll.is_closed);
    }

    /// All coins tie on (total_likes=0, last_posted_at): without a unique final sort
    /// key, OFFSET paging can repeat one coin across pages and drop another.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_pages_are_disjoint_when_sort_keys_tie(pool: sqlx::PgPool) {
        let c = ctl(pool.clone());
        for i in 0..6 {
            let token_id = format!("0xTie{i}");
            seed_token(&pool, &token_id, "0xCreator").await;
            c.create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id,
                    title: "Announcement".into(),
                    body: Some("tie".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        }
        // force an exact tie on last_posted_at too (creation order would otherwise break it)
        sqlx::query("UPDATE dev_post SET created_at = TIMESTAMPTZ '2026-01-01 00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();

        let mut seen: Vec<String> = Vec::new();
        for page in 1..=3 {
            let (rows, total) = c.get_ranking(page, 2).await.unwrap();
            assert_eq!(total, 6);
            seen.extend(rows.into_iter().map(|r| r.token.token_id));
        }
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            6,
            "pages overlapped or skipped coins: {seen:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_and_trending_by_likes(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let p = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("p".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        c.like(p, "0xU1").await.unwrap();
        c.like(p, "0xU2").await.unwrap();
        let (rank, total) = c.get_ranking(1, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rank[0].total_likes, 2);
        assert_eq!(rank[0].post_count, 1);
        let trending = c.get_trending(None).await.unwrap();
        assert_eq!(trending.len(), 1);
        assert_eq!(trending[0].id, p.to_string());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn trending_falls_back_to_newest_when_no_likes(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let mut ids = Vec::new();
        for i in 0..4 {
            let id = c
                .create_post(
                    "0xCreator",
                    &CreateDevPostRequest {
                        token_id: "0xToken".into(),
                        title: "Announcement".into(),
                        body: Some(format!("post {i}")),
                        image_uris: None,
                        poll: None,
                    },
                )
                .await
                .unwrap();
            ids.push(id);
        }
        let trending = c.get_trending(None).await.unwrap();
        let want: Vec<String> = ids[1..].iter().rev().map(|id| id.to_string()).collect();
        let got: Vec<String> = trending.into_iter().map(|p| p.id).collect();
        assert_eq!(got, want);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hydrate_base_has_no_personalization(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        c.like(id, "0xU").await.unwrap();
        c.vote(id, "0xU", 2).await.unwrap();
        let base = c.hydrate_base(c.db.get_read_pool(), &[id]).await.unwrap();
        assert!(!base[0].liked_by_me);
        assert_eq!(base[0].poll.as_ref().unwrap().my_vote_option, None);
        assert_eq!(base[0].like_count, 1); // counts ARE in base
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn apply_personalization_overlays_viewer(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = create_poll_post(&c).await;
        c.like(id, "0xU").await.unwrap();
        c.vote(id, "0xU", 2).await.unwrap();

        let mut base = vec![c.get_post_base(id).await.unwrap()];
        c.apply_personalization(&mut base, "0xU").await.unwrap();
        assert!(base[0].liked_by_me);
        assert_eq!(base[0].poll.as_ref().unwrap().my_vote_option, Some(2));

        // a different viewer sees nothing
        let mut base2 = vec![c.get_post_base(id).await.unwrap()];
        c.apply_personalization(&mut base2, "0xOther")
            .await
            .unwrap();
        assert!(!base2[0].liked_by_me);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_and_delete_return_token_id(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let id = c
            .create_post(
                "0xCreator",
                &CreateDevPostRequest {
                    token_id: "0xToken".into(),
                    title: "Announcement".into(),
                    body: Some("b".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            c.edit_post(
                id,
                "0xCreator",
                &EditDevPostRequest {
                    title: Default::default(),
                    body: Some("b2".into()),
                    image_uris: None,
                },
            )
            .await
            .unwrap(),
            "0xToken"
        );
        let outcome = c.delete_post(id, "0xCreator").await.unwrap();
        match outcome {
            moderation::CommitOutcome::Committed(context) => {
                assert_eq!(context.token_id, "0xToken");
                assert!(context.changed);
            }
            moderation::CommitOutcome::Unknown { .. } => panic!("delete commit outcome unknown"),
        }
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_populates_market_cap_usd(pool: sqlx::PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        set_total_supply(&pool, TOKEN, "1000000000000000000000000").await; // 1,000,000 tokens, raw ×10^18
        seed_market(&pool, TOKEN, "2").await;
        seed_quote_usd_price(&pool, "0.5").await;
        let c = ctl(pool.clone());
        c.create_post(
            CREATOR,
            &CreateDevPostRequest {
                token_id: TOKEN.into(),
                title: "Announcement".into(),
                body: Some("x".into()),
                image_uris: None,
                poll: None,
            },
        )
        .await
        .unwrap();
        let (rank, _total) = c.get_ranking(1, 10).await.unwrap();
        assert_eq!(rank[0].token.market_cap, Some("1000000".to_string()));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hydrate_populates_market_cap_usd(pool: sqlx::PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        set_total_supply(&pool, TOKEN, "1000000").await;
        seed_market(&pool, TOKEN, "2").await;
        seed_quote_usd_price(&pool, "0.5").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                CREATOR,
                &CreateDevPostRequest {
                    token_id: TOKEN.into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let post = c.get_post_rw(id, None).await.unwrap();
        assert_eq!(post.token.market_cap, Some("1000000".to_string()));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn market_cap_zero_when_quote_price_missing(pool: sqlx::PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        set_total_supply(&pool, TOKEN, "1000000").await;
        seed_market(&pool, TOKEN, "2").await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                CREATOR,
                &CreateDevPostRequest {
                    token_id: TOKEN.into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let post = c.get_post_rw(id, None).await.unwrap();
        assert_eq!(post.token.market_cap, Some("0".to_string()));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn market_cap_null_when_no_market_row(pool: sqlx::PgPool) {
        seed_token(&pool, TOKEN, CREATOR).await;
        let c = ctl(pool.clone());
        let id = c
            .create_post(
                CREATOR,
                &CreateDevPostRequest {
                    token_id: TOKEN.into(),
                    title: "Announcement".into(),
                    body: Some("x".into()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        let post = c.get_post_rw(id, None).await.unwrap();
        assert_eq!(post.token.market_cap, None);
    }
}
