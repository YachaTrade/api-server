use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use tracing::{error, info};

use crate::{
    db::postgres::PostgresDatabase,
    services::github::{GitHubCreatorInfo, GitHubProjectInfo},
    types::hackathon::{
        HackathonInfo, HackathonMemberGitHubInfo, HackathonProjectInfo, HackathonTeamInfo,
        HackathonTeamMemberInfo, TeamMemberInput,
    },
};

// ===== DB Row Types =====

#[derive(Debug, sqlx::FromRow)]
struct TeamMemberRow {
    team_id: i64,
    email: String,
    discord: Option<String>,
    github_username: Option<String>,
    twitter: Option<String>,
    linkedin: Option<String>,
    github_image_uri: Option<String>,
    github_name: Option<String>,
    github_url: Option<String>,
    github_follower_count: Option<i32>,
    github_following_count: Option<i32>,
    github_repo_count: Option<i32>,
    github_star_count: Option<i32>,
    github_bio: Option<String>,
    github_fetched_at: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectWithTeamRow {
    // Project fields
    token_id: String,
    team_id: i64,
    project_name: String,
    project_description: String,
    monad_integration: String,
    github_url: String,
    demo_video_url: String,
    agent_moltbook_url: Option<String>,
    github_star_count: Option<i32>,
    github_fork_count: Option<i32>,
    github_description: Option<String>,
    github_topics: Option<String>,
    github_language: Option<String>,
    // Team fields
    team_name: String,
}

// ===== Controller =====

pub struct HackathonController {
    db: Arc<PostgresDatabase>,
}

/// Parameters for hackathon registration
pub struct RegisterHackathonParams<'a> {
    pub token_id: &'a str,
    pub team_name: &'a str,
    pub members: &'a [TeamMemberInput],
    pub project_name: &'a str,
    pub project_description: &'a str,
    pub monad_integration: &'a str,
    pub project_github_url: &'a str,
    pub demo_video_url: &'a str,
    pub agent_moltbook_url: Option<&'a str>,
    pub project_github_info: Option<&'a GitHubProjectInfo>,
    pub members_github_info: &'a HashMap<String, GitHubCreatorInfo>,
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

    /// Check if hackathon already exists for token
    pub async fn hackathon_exists(&self, token_id: &str) -> Result<bool> {
        let query = "SELECT 1 FROM hackathon WHERE token_id = $1";
        let result: Option<(i32,)> = sqlx::query_as(query)
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
            .await?;
        Ok(result.is_some())
    }

    /// Register hackathon (single query for pgbouncer compatibility)
    /// Returns Some((team_id, github_fetch_pending)) if registered, None if already exists
    pub async fn register_hackathon_tx(
        &self,
        params: RegisterHackathonParams<'_>,
    ) -> Result<Option<(i64, bool)>> {
        // Check if already exists
        if self.hackathon_exists(params.token_id).await? {
            return Ok(None);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut github_fetch_pending = false;
        let pool = self.db.get_write_pool();

        // Prepare member data
        let mut member_emails: Vec<String> = Vec::new();
        let mut member_discords: Vec<Option<String>> = Vec::new();
        let mut member_github_usernames: Vec<Option<String>> = Vec::new();
        let mut member_twitters: Vec<Option<String>> = Vec::new();
        let mut member_linkedins: Vec<Option<String>> = Vec::new();
        let mut member_github_image_uris: Vec<Option<String>> = Vec::new();
        let mut member_github_names: Vec<Option<String>> = Vec::new();
        let mut member_github_urls: Vec<Option<String>> = Vec::new();
        let mut member_github_follower_counts: Vec<Option<i32>> = Vec::new();
        let mut member_github_following_counts: Vec<Option<i32>> = Vec::new();
        let mut member_github_repo_counts: Vec<Option<i32>> = Vec::new();
        let mut member_github_star_counts: Vec<Option<i32>> = Vec::new();
        let mut member_github_bios: Vec<Option<String>> = Vec::new();
        let mut member_github_fetched_ats: Vec<Option<i64>> = Vec::new();

        for member in params.members {
            let github_username = member
                .github_username
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string());

            let github_info = github_username
                .as_ref()
                .and_then(|username| params.members_github_info.get(&username.to_lowercase()));

            member_emails.push(member.email.trim().to_lowercase());
            member_discords.push(member.discord.clone());
            member_github_usernames.push(github_username.clone());
            member_twitters.push(member.twitter.clone());
            member_linkedins.push(member.linkedin.clone());

            if let Some(info) = github_info {
                member_github_image_uris.push(info.image_uri.clone());
                member_github_names.push(info.name.clone());
                member_github_urls.push(Some(info.github_url.clone()));
                member_github_follower_counts.push(Some(info.follower_count));
                member_github_following_counts.push(Some(info.following_count));
                member_github_repo_counts.push(Some(info.repo_count));
                member_github_star_counts.push(Some(info.star_count));
                member_github_bios.push(info.bio.clone());
                member_github_fetched_ats.push(Some(now));
            } else {
                if github_username.is_some() {
                    github_fetch_pending = true;
                }
                member_github_image_uris.push(None);
                member_github_names.push(None);
                member_github_urls.push(None);
                member_github_follower_counts.push(None);
                member_github_following_counts.push(None);
                member_github_repo_counts.push(None);
                member_github_star_counts.push(None);
                member_github_bios.push(None);
                member_github_fetched_ats.push(None);
            }
        }

        // Prepare project GitHub data
        let (
            project_github_star_count,
            project_github_fork_count,
            project_github_description,
            project_github_topics,
            project_github_language,
            project_github_fetched_at,
        ) = if let Some(info) = params.project_github_info {
            (
                Some(info.star_count),
                Some(info.fork_count),
                info.description.clone(),
                info.topics.as_ref().map(|t| t.join(",")),
                info.language.clone(),
                Some(now),
            )
        } else {
            github_fetch_pending = true;
            (None, None, None, None, None, None)
        };

        // Single CTE query for all inserts
        let query = r#"
            WITH ins_hackathon AS (
                INSERT INTO hackathon (token_id)
                VALUES ($1)
                ON CONFLICT (token_id) DO NOTHING
                RETURNING token_id
            ),
            ins_team AS (
                INSERT INTO hackathon_team (name, created_at, updated_at)
                SELECT $2, $3, $3
                WHERE EXISTS (SELECT 1 FROM ins_hackathon)
                RETURNING id
            ),
            ins_members AS (
                INSERT INTO hackathon_team_member (
                    team_id, email, discord, github_username, twitter, linkedin,
                    github_image_uri, github_name, github_url,
                    github_follower_count, github_following_count, github_repo_count,
                    github_star_count, github_bio, github_fetched_at,
                    created_at, updated_at
                )
                SELECT
                    t.id,
                    m.email, m.discord, m.github_username, m.twitter, m.linkedin,
                    m.github_image_uri, m.github_name, m.github_url,
                    m.github_follower_count, m.github_following_count, m.github_repo_count,
                    m.github_star_count, m.github_bio, m.github_fetched_at,
                    $3, $3
                FROM ins_team t,
                UNNEST(
                    $4::TEXT[], $5::TEXT[], $6::TEXT[], $7::TEXT[], $8::TEXT[],
                    $9::TEXT[], $10::TEXT[], $11::TEXT[],
                    $12::INTEGER[], $13::INTEGER[], $14::INTEGER[],
                    $15::INTEGER[], $16::TEXT[], $17::BIGINT[]
                ) AS m(
                    email, discord, github_username, twitter, linkedin,
                    github_image_uri, github_name, github_url,
                    github_follower_count, github_following_count, github_repo_count,
                    github_star_count, github_bio, github_fetched_at
                )
                RETURNING team_id
            ),
            ins_project AS (
                INSERT INTO hackathon_project (
                    token_id, team_id, name, description, monad_integration,
                    github_url, demo_video_url, agent_moltbook_url,
                    github_star_count, github_fork_count, github_description,
                    github_topics, github_language, github_fetched_at,
                    created_at, updated_at
                )
                SELECT
                    $1, t.id, $18, $19, $20, $21, $22, $23,
                    $24, $25, $26, $27, $28, $29, $3, $3
                FROM ins_team t
                RETURNING team_id
            )
            SELECT id FROM ins_team
        "#;

        let result: Option<(i64,)> = sqlx::query_as(query)
            .bind(params.token_id)           // $1
            .bind(params.team_name)          // $2
            .bind(now)                       // $3
            .bind(&member_emails)            // $4
            .bind(&member_discords)          // $5
            .bind(&member_github_usernames)  // $6
            .bind(&member_twitters)          // $7
            .bind(&member_linkedins)         // $8
            .bind(&member_github_image_uris) // $9
            .bind(&member_github_names)      // $10
            .bind(&member_github_urls)       // $11
            .bind(&member_github_follower_counts)  // $12
            .bind(&member_github_following_counts) // $13
            .bind(&member_github_repo_counts)      // $14
            .bind(&member_github_star_counts)      // $15
            .bind(&member_github_bios)       // $16
            .bind(&member_github_fetched_ats) // $17
            .bind(params.project_name)       // $18
            .bind(params.project_description) // $19
            .bind(params.monad_integration)  // $20
            .bind(params.project_github_url) // $21
            .bind(params.demo_video_url)     // $22
            .bind(params.agent_moltbook_url) // $23
            .bind(project_github_star_count) // $24
            .bind(project_github_fork_count) // $25
            .bind(&project_github_description) // $26
            .bind(&project_github_topics)    // $27
            .bind(&project_github_language)  // $28
            .bind(project_github_fetched_at) // $29
            .fetch_optional(pool)
            .await?;

        match result {
            Some((team_id,)) => {
                info!(
                    "Registered hackathon project: {} (team_id: {})",
                    params.token_id, team_id
                );
                Ok(Some((team_id, github_fetch_pending)))
            }
            None => Ok(None), // Already exists (race condition)
        }
    }

    /// Get hackathon info for a single token
    pub async fn get_hackathon_info(&self, token_id: &str) -> Option<HackathonInfo> {
        // Step 1: Get project with team
        let project_query = r#"
            SELECT
                p.token_id,
                p.team_id,
                p.name as project_name,
                p.description as project_description,
                p.monad_integration,
                p.github_url,
                p.demo_video_url,
                p.agent_moltbook_url,
                p.github_star_count,
                p.github_fork_count,
                p.github_description,
                p.github_topics,
                p.github_language,
                t.name as team_name
            FROM hackathon_project p
            JOIN hackathon_team t ON p.team_id = t.id
            WHERE p.token_id = $1
        "#;

        let project: ProjectWithTeamRow = sqlx::query_as(project_query)
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
            .await
            .ok()??;

        // Step 2: Get team members
        let members_query = r#"
            SELECT team_id, email, discord, github_username, twitter, linkedin,
                   github_image_uri, github_name, github_url,
                   github_follower_count, github_following_count, github_repo_count,
                   github_star_count, github_bio, github_fetched_at
            FROM hackathon_team_member
            WHERE team_id = $1
        "#;

        let members: Vec<TeamMemberRow> = sqlx::query_as(members_query)
            .bind(project.team_id)
            .fetch_all(self.db.get_read_pool())
            .await
            .ok()?;

        Some(self.build_hackathon_info(project, members))
    }

    /// Get hackathon infos for multiple tokens (batch query)
    pub async fn get_hackathon_infos_by_token_ids(
        &self,
        token_ids: &[String],
    ) -> HashMap<String, HackathonInfo> {
        if token_ids.is_empty() {
            return HashMap::new();
        }

        // Step 1: Query projects with teams
        let placeholders: Vec<String> = (1..=token_ids.len()).map(|i| format!("${}", i)).collect();
        let project_query = format!(
            r#"
            SELECT
                p.token_id,
                p.team_id,
                p.name as project_name,
                p.description as project_description,
                p.monad_integration,
                p.github_url,
                p.demo_video_url,
                p.agent_moltbook_url,
                p.github_star_count,
                p.github_fork_count,
                p.github_description,
                p.github_topics,
                p.github_language,
                t.name as team_name
            FROM hackathon_project p
            JOIN hackathon_team t ON p.team_id = t.id
            WHERE p.token_id IN ({})
            "#,
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, ProjectWithTeamRow>(&project_query);
        for token_id in token_ids {
            query_builder = query_builder.bind(token_id);
        }

        let projects = match query_builder.fetch_all(self.db.get_read_pool()).await {
            Ok(rows) => rows,
            Err(e) => {
                error!("Failed to fetch hackathon projects: {}", e);
                return HashMap::new();
            }
        };

        if projects.is_empty() {
            return HashMap::new();
        }

        // Step 2: Collect unique team_ids
        let team_ids: Vec<i64> = projects
            .iter()
            .map(|p| p.team_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Step 3: Query all members for these teams
        let member_placeholders: Vec<String> =
            (1..=team_ids.len()).map(|i| format!("${}", i)).collect();
        let members_query = format!(
            r#"
            SELECT team_id, email, discord, github_username, twitter, linkedin,
                   github_image_uri, github_name, github_url,
                   github_follower_count, github_following_count, github_repo_count,
                   github_star_count, github_bio, github_fetched_at
            FROM hackathon_team_member
            WHERE team_id IN ({})
            "#,
            member_placeholders.join(", ")
        );

        let mut member_query_builder = sqlx::query_as::<_, TeamMemberRow>(&members_query);
        for team_id in &team_ids {
            member_query_builder = member_query_builder.bind(team_id);
        }

        let members = match member_query_builder.fetch_all(self.db.get_read_pool()).await {
            Ok(rows) => rows,
            Err(e) => {
                error!("Failed to fetch hackathon team members: {}", e);
                return HashMap::new();
            }
        };

        // Step 4: Group members by team_id
        let mut members_by_team: HashMap<i64, Vec<TeamMemberRow>> = HashMap::new();
        for member in members {
            members_by_team
                .entry(member.team_id)
                .or_default()
                .push(member);
        }

        // Step 5: Assemble final result
        let mut result: HashMap<String, HackathonInfo> = HashMap::new();
        for project in projects {
            let team_members = members_by_team
                .remove(&project.team_id)
                .unwrap_or_default();

            let token_id = project.token_id.clone();
            let hackathon_info = self.build_hackathon_info(project, team_members);
            result.insert(token_id, hackathon_info);
        }

        result
    }

    /// Build HackathonInfo from DB rows
    fn build_hackathon_info(
        &self,
        project: ProjectWithTeamRow,
        members: Vec<TeamMemberRow>,
    ) -> HackathonInfo {
        let team_members: Vec<HackathonTeamMemberInfo> = members
            .into_iter()
            .map(|m| {
                let github = m.github_username.as_ref().map(|username| {
                    HackathonMemberGitHubInfo {
                        username: username.clone(),
                        image_uri: m.github_image_uri.clone().unwrap_or_default(),
                        name: m.github_name.clone().unwrap_or_default(),
                        url: m.github_url.clone().unwrap_or_default(),
                        follower_count: m.github_follower_count.unwrap_or_default(),
                        following_count: m.github_following_count.unwrap_or_default(),
                        repo_count: m.github_repo_count.unwrap_or_default(),
                        star_count: m.github_star_count.unwrap_or_default(),
                        bio: m.github_bio.clone(),
                        fetch_pending: m.github_fetched_at.is_none(),
                    }
                });

                HackathonTeamMemberInfo {
                    email: m.email,
                    discord: m.discord,
                    twitter: m.twitter,
                    linkedin: m.linkedin,
                    github,
                }
            })
            .collect();

        let topics: Option<Vec<String>> = project.github_topics.map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        });

        HackathonInfo {
            team: HackathonTeamInfo {
                id: project.team_id.to_string(),
                name: project.team_name,
                members: team_members,
            },
            project: HackathonProjectInfo {
                name: project.project_name,
                description: project.project_description,
                monad_integration: project.monad_integration,
                github_url: project.github_url,
                demo_video_url: project.demo_video_url,
                agent_moltbook_url: project.agent_moltbook_url,
                github_star_count: project.github_star_count.unwrap_or(0),
                github_fork_count: project.github_fork_count.unwrap_or(0),
                github_description: project.github_description,
                github_topics: topics,
                github_language: project.github_language,
            },
        }
    }

    /// Get project's github_fetched_at timestamp
    pub async fn get_project_github_fetched_at(&self, token_id: &str) -> Option<i64> {
        let query = "SELECT github_fetched_at FROM hackathon_project WHERE token_id = $1";
        let result: Option<(Option<i64>,)> = sqlx::query_as(query)
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
            .await
            .ok()?;
        result.and_then(|(ts,)| ts)
    }

    /// Update GitHub info for project
    pub async fn update_project_github_info(
        &self,
        token_id: &str,
        info: &GitHubProjectInfo,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let topics_str = info.topics.as_ref().map(|t| t.join(","));

        let query = r#"
            UPDATE hackathon_project SET
                github_star_count = $2,
                github_fork_count = $3,
                github_description = $4,
                github_topics = $5,
                github_language = $6,
                github_fetched_at = $7,
                updated_at = $7
            WHERE token_id = $1
        "#;

        sqlx::query(query)
            .bind(token_id)
            .bind(info.star_count)
            .bind(info.fork_count)
            .bind(&info.description)
            .bind(&topics_str)
            .bind(&info.language)
            .bind(now)
            .execute(self.db.get_write_pool())
            .await?;

        Ok(())
    }

    /// Update GitHub info for team member
    pub async fn update_member_github_info(
        &self,
        member_id: i64,
        info: &GitHubCreatorInfo,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let query = r#"
            UPDATE hackathon_team_member SET
                github_image_uri = $2,
                github_name = $3,
                github_url = $4,
                github_follower_count = $5,
                github_following_count = $6,
                github_repo_count = $7,
                github_star_count = $8,
                github_bio = $9,
                github_fetched_at = $10,
                updated_at = $10
            WHERE id = $1
        "#;

        sqlx::query(query)
            .bind(member_id)
            .bind(&info.image_uri)
            .bind(&info.name)
            .bind(&info.github_url)
            .bind(info.follower_count)
            .bind(info.following_count)
            .bind(info.repo_count)
            .bind(info.star_count)
            .bind(&info.bio)
            .bind(now)
            .execute(self.db.get_write_pool())
            .await?;

        Ok(())
    }

    /// Get members that need GitHub refresh for a token
    /// Returns Vec<(member_id, github_username)>
    pub async fn get_members_needing_refresh(
        &self,
        token_id: &str,
        stale_threshold: i64,
    ) -> Vec<(i64, String)> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let query = r#"
            SELECT m.id, m.github_username
            FROM hackathon_team_member m
            JOIN hackathon_project p ON m.team_id = p.team_id
            WHERE p.token_id = $1
              AND m.github_username IS NOT NULL
              AND (m.github_fetched_at IS NULL OR $2 - m.github_fetched_at > $3)
        "#;

        sqlx::query_as::<_, (i64, String)>(query)
            .bind(token_id)
            .bind(now)
            .bind(stale_threshold)
            .fetch_all(self.db.get_read_pool())
            .await
            .unwrap_or_default()
    }
}
