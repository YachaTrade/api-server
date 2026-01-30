use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use tracing::info;

use crate::{
    db::postgres::PostgresDatabase,
    services::github::{GitHubCreatorInfo, GitHubProjectInfo},
    types::hackathon::{HackathonCreatorInfo, HackathonInfo, HackathonProjectInfo},
};

#[derive(Debug, sqlx::FromRow)]
struct HackathonTokenRow {
    token_id: String,
    // Creator fields
    github_id: String,
    creator_image_uri: Option<String>,
    creator_name: Option<String>,
    creator_github_url: Option<String>,
    follower_count: i32,
    following_count: i32,
    repo_count: i32,
    creator_star_count: i32,
    bio: Option<String>,
    twitter: String,
    discord: Option<String>,
    telegram: Option<String>,
    linkedin: Option<String>,
    account_id: String,
    // Project fields
    project_github_url: String,
    project_name: String,
    description: String,
    keywords: String,
    screenshot_uri: String,
    website: Option<String>,
    youtube: Option<String>,
    project_star_count: i32,
    fork_count: i32,
    topics: Option<String>,
    language: Option<String>,
    // Timestamps
    project_updated_at: i64,
}

pub struct HackathonController {
    db: Arc<PostgresDatabase>,
}

/// Parameters for hackathon registration
pub struct RegisterHackathonParams<'a> {
    pub token_id: &'a str,
    pub github_id: &'a str,
    pub creator_info: &'a GitHubCreatorInfo,
    pub twitter: &'a str,
    pub discord: Option<&'a str>,
    pub telegram: Option<&'a str>,
    pub linkedin: Option<&'a str>,
    pub account_id: &'a str,
    pub project_github_url: &'a str,
    pub project_name: &'a str,
    pub project_description: &'a str,
    pub keywords: &'a str,
    pub screenshot_uri: &'a str,
    pub website: Option<&'a str>,
    pub youtube: Option<&'a str>,
    pub project_info: &'a GitHubProjectInfo,
}

impl HackathonController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Check if token exists in token table
    pub async fn token_exists(&self, token_id: &str) -> Result<bool> {
        let query = "SELECT 1 FROM token WHERE token_id = $1";
        let result: Option<(i32,)> = sqlx::query_as(query)
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
            .await?;
        Ok(result.is_some())
    }

    /// Get all hackathon token_ids
    pub async fn get_hackathon_token_ids(&self) -> Result<Vec<String>> {
        let query = "SELECT token_id FROM hackathon ORDER BY created_at DESC";
        let rows: Vec<(String,)> = sqlx::query_as(query)
            .fetch_all(self.db.get_read_pool())
            .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Register hackathon with transaction (atomic operation)
    /// 1. Insert hackathon (whitelist)
    /// 2. Upsert creator
    /// 3. Insert/update project
    pub async fn register_hackathon_tx(&self, params: RegisterHackathonParams<'_>) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut tx = self.db.get_write_pool().begin().await?;

        // 1. Insert hackathon (whitelist entry)
        let hackathon_query = r#"
            INSERT INTO hackathon (token_id)
            VALUES ($1)
            ON CONFLICT (token_id) DO NOTHING
        "#;
        sqlx::query(hackathon_query)
            .bind(params.token_id)
            .execute(&mut *tx)
            .await?;

        // 2. Upsert creator
        let creator_query = r#"
            INSERT INTO hackathon_creator (
                github_id, image_uri, name, github_url,
                follower_count, following_count, repo_count, star_count, bio,
                twitter, discord, telegram, linkedin, account_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (github_id) DO UPDATE SET
                image_uri = EXCLUDED.image_uri,
                name = EXCLUDED.name,
                github_url = EXCLUDED.github_url,
                follower_count = EXCLUDED.follower_count,
                following_count = EXCLUDED.following_count,
                repo_count = EXCLUDED.repo_count,
                star_count = EXCLUDED.star_count,
                bio = EXCLUDED.bio,
                twitter = EXCLUDED.twitter,
                discord = EXCLUDED.discord,
                telegram = EXCLUDED.telegram,
                linkedin = EXCLUDED.linkedin,
                account_id = EXCLUDED.account_id,
                updated_at = EXCLUDED.updated_at
        "#;
        sqlx::query(creator_query)
            .bind(params.github_id)
            .bind(&params.creator_info.image_uri)
            .bind(&params.creator_info.name)
            .bind(&params.creator_info.github_url)
            .bind(params.creator_info.follower_count)
            .bind(params.creator_info.following_count)
            .bind(params.creator_info.repo_count)
            .bind(params.creator_info.star_count)
            .bind(&params.creator_info.bio)
            .bind(params.twitter)
            .bind(params.discord)
            .bind(params.telegram)
            .bind(params.linkedin)
            .bind(params.account_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        // 3. Insert/update project
        let topics_str = params.project_info.topics.as_ref().map(|t| t.join(","));
        let project_query = r#"
            INSERT INTO hackathon_project (
                token_id, github_id, github_url, name, description,
                keywords, screenshot_uri, website, youtube,
                star_count, fork_count, topics, language, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (token_id) DO UPDATE SET
                github_id = EXCLUDED.github_id,
                github_url = EXCLUDED.github_url,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                keywords = EXCLUDED.keywords,
                screenshot_uri = EXCLUDED.screenshot_uri,
                website = EXCLUDED.website,
                youtube = EXCLUDED.youtube,
                star_count = EXCLUDED.star_count,
                fork_count = EXCLUDED.fork_count,
                topics = EXCLUDED.topics,
                language = EXCLUDED.language,
                updated_at = EXCLUDED.updated_at
        "#;
        sqlx::query(project_query)
            .bind(params.token_id)
            .bind(params.github_id)
            .bind(params.project_github_url)
            .bind(params.project_name)
            .bind(params.project_description)
            .bind(params.keywords)
            .bind(params.screenshot_uri)
            .bind(params.website)
            .bind(params.youtube)
            .bind(params.project_info.star_count)
            .bind(params.project_info.fork_count)
            .bind(&topics_str)
            .bind(&params.project_info.language)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        info!(
            "Registered hackathon project in transaction: {}",
            params.token_id
        );
        Ok(())
    }

    /// Get hackathon info for a single token with updated_at timestamp
    /// Returns (Option<HackathonInfo>, Option<updated_at>)
    pub async fn get_hackathon_info_with_timestamp(
        &self,
        token_id: &str,
    ) -> Option<(HackathonInfo, i64)> {
        let query = r#"
            SELECT
                hp.token_id,
                hc.github_id,
                hc.image_uri as creator_image_uri,
                hc.name as creator_name,
                hc.github_url as creator_github_url,
                hc.follower_count,
                hc.following_count,
                hc.repo_count,
                hc.star_count as creator_star_count,
                hc.bio,
                hc.twitter,
                hc.discord,
                hc.telegram,
                hc.linkedin,
                hc.account_id,
                hp.github_url as project_github_url,
                hp.name as project_name,
                hp.description,
                hp.keywords,
                hp.screenshot_uri,
                hp.website,
                hp.youtube,
                hp.star_count as project_star_count,
                hp.fork_count,
                hp.topics,
                hp.language,
                hp.updated_at as project_updated_at
            FROM hackathon_project hp
            JOIN hackathon_creator hc ON hp.github_id = hc.github_id
            WHERE hp.token_id = $1
        "#;

        let row = sqlx::query_as::<_, HackathonTokenRow>(query)
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
            .await
            .ok()?;

        row.map(|r| {
            let updated_at = r.project_updated_at;
            (HackathonInfo::from(r), updated_at)
        })
    }

    /// Get hackathon info for a single token
    /// Returns None if not found or on error (fail-safe)
    pub async fn get_hackathon_info(&self, token_id: &str) -> Option<HackathonInfo> {
        self.get_hackathon_info_with_timestamp(token_id)
            .await
            .map(|(info, _)| info)
    }

    /// Get hackathon infos for multiple tokens (batch query)
    /// Returns a HashMap<token_id, HackathonInfo> for easy lookup
    /// Returns empty HashMap on error (fail-safe)
    pub async fn get_hackathon_infos_by_token_ids(
        &self,
        token_ids: &[String],
    ) -> HashMap<String, HackathonInfo> {
        if token_ids.is_empty() {
            return HashMap::new();
        }

        let placeholders: Vec<String> = (1..=token_ids.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            r#"
            SELECT
                hp.token_id,
                hc.github_id,
                hc.image_uri as creator_image_uri,
                hc.name as creator_name,
                hc.github_url as creator_github_url,
                hc.follower_count,
                hc.following_count,
                hc.repo_count,
                hc.star_count as creator_star_count,
                hc.bio,
                hc.twitter,
                hc.discord,
                hc.telegram,
                hc.linkedin,
                hc.account_id,
                hp.github_url as project_github_url,
                hp.name as project_name,
                hp.description,
                hp.keywords,
                hp.screenshot_uri,
                hp.website,
                hp.youtube,
                hp.star_count as project_star_count,
                hp.fork_count,
                hp.topics,
                hp.language,
                hp.updated_at as project_updated_at
            FROM hackathon_project hp
            JOIN hackathon_creator hc ON hp.github_id = hc.github_id
            WHERE hp.token_id IN ({})
            "#,
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, HackathonTokenRow>(&query);
        for token_id in token_ids {
            query_builder = query_builder.bind(token_id);
        }

        let rows = match query_builder.fetch_all(self.db.get_read_pool()).await {
            Ok(rows) => rows,
            Err(_) => return HashMap::new(),
        };

        rows.into_iter()
            .map(|row| {
                let token_id = row.token_id.clone();
                (token_id, HackathonInfo::from(row))
            })
            .collect()
    }

    /// Update GitHub info for creator and project
    /// Called when data is stale (>1 hour since last update)
    pub async fn update_github_info_tx(
        &self,
        token_id: &str,
        github_id: &str,
        creator_info: &GitHubCreatorInfo,
        project_info: &GitHubProjectInfo,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut tx = self.db.get_write_pool().begin().await?;

        // Update creator
        let creator_query = r#"
            UPDATE hackathon_creator SET
                image_uri = $2,
                name = $3,
                github_url = $4,
                follower_count = $5,
                following_count = $6,
                repo_count = $7,
                star_count = $8,
                bio = $9,
                updated_at = $10
            WHERE github_id = $1
        "#;
        sqlx::query(creator_query)
            .bind(github_id)
            .bind(&creator_info.image_uri)
            .bind(&creator_info.name)
            .bind(&creator_info.github_url)
            .bind(creator_info.follower_count)
            .bind(creator_info.following_count)
            .bind(creator_info.repo_count)
            .bind(creator_info.star_count)
            .bind(&creator_info.bio)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        // Update project
        let topics_str = project_info.topics.as_ref().map(|t| t.join(","));
        let project_query = r#"
            UPDATE hackathon_project SET
                star_count = $2,
                fork_count = $3,
                topics = $4,
                language = $5,
                updated_at = $6
            WHERE token_id = $1
        "#;
        sqlx::query(project_query)
            .bind(token_id)
            .bind(project_info.star_count)
            .bind(project_info.fork_count)
            .bind(&topics_str)
            .bind(&project_info.language)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        info!(
            "Updated GitHub info for hackathon project: {} (github_id: {})",
            token_id, github_id
        );
        Ok(())
    }
}

impl From<HackathonTokenRow> for HackathonInfo {
    fn from(row: HackathonTokenRow) -> Self {
        let keywords: Vec<String> = row
            .keywords
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let topics: Option<Vec<String>> = row.topics.map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        });

        Self {
            creator: HackathonCreatorInfo {
                github_id: row.github_id,
                image_uri: row.creator_image_uri,
                name: row.creator_name,
                github_url: row.creator_github_url,
                follower_count: row.follower_count,
                following_count: row.following_count,
                repo_count: row.repo_count,
                star_count: row.creator_star_count,
                bio: row.bio,
                twitter: row.twitter,
                discord: row.discord,
                telegram: row.telegram,
                linkedin: row.linkedin,
                account_id: row.account_id,
            },
            project: HackathonProjectInfo {
                github_url: row.project_github_url,
                name: row.project_name,
                description: row.description,
                keywords,
                screenshot_uri: row.screenshot_uri,
                website: row.website,
                youtube: row.youtube,
                star_count: row.project_star_count,
                fork_count: row.fork_count,
                topics,
                language: row.language,
            },
        }
    }
}
