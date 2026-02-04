use std::{collections::HashMap, sync::Arc};

use tracing::{error, info, warn};

use crate::{
    config::HACKATHON_REFRESH_INTERVAL_SECS,
    controllers::hackathon::{HackathonController, RegisterHackathonParams},
    db::postgres::PostgresDatabase,
    services::github::{GitHubCreatorInfo, GitHubService},
    types::hackathon::{HackathonInfo, RegisterHackathonRequest, TeamMemberInput},
};

pub struct HackathonService {
    postgres: Arc<PostgresDatabase>,
}

impl HackathonService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    /// Register a new hackathon project
    /// Returns (team_id, github_fetch_pending)
    pub async fn register_hackathon(
        &self,
        request: &RegisterHackathonRequest,
    ) -> Result<(String, bool), String> {
        let controller = HackathonController::new(self.postgres.clone());

        // Verify token exists
        match controller.token_exists(&request.token_id).await {
            Ok(true) => {}
            Ok(false) => return Err("Token not found".to_string()),
            Err(e) => return Err(format!("Database error: {}", e)),
        }

        // Fetch GitHub info (project + members in parallel)
        let project_github_url = request.project_github_url.clone();
        let members_clone = request.members.clone();

        let (project_github_info, members_github_info) = tokio::join!(
            async {
                let github_service = GitHubService::new();
                match github_service.get_project_info(&project_github_url).await {
                    Ok(info) => Some(info),
                    Err(e) => {
                        warn!(
                            "Failed to fetch project GitHub info for {}: {}",
                            project_github_url, e
                        );
                        None
                    }
                }
            },
            self.fetch_members_github_info(&members_clone)
        );

        // Register in database
        let params = RegisterHackathonParams {
            token_id: &request.token_id,
            team_name: &request.team_name,
            members: &request.members,
            project_name: &request.project_name,
            project_description: &request.project_description,
            monad_integration: &request.monad_integration,
            project_github_url: &request.project_github_url,
            demo_video_url: &request.demo_video_url,
            agent_moltbook_url: request.agent_moltbook_url.as_deref(),
            screenshot_uri: request.screenshot_uri.as_deref(),
            website: request.website.as_deref(),
            project_github_info: project_github_info.as_ref(),
            members_github_info: &members_github_info,
        };

        match controller.register_hackathon_tx(params).await {
            Ok((team_id, github_fetch_pending)) => Ok((team_id.to_string(), github_fetch_pending)),
            Err(e) => Err(format!("Failed to register hackathon: {}", e)),
        }
    }

    /// Fetch GitHub info for all members with github_username
    async fn fetch_members_github_info(
        &self,
        members: &[TeamMemberInput],
    ) -> HashMap<String, GitHubCreatorInfo> {
        let usernames: Vec<&str> = members
            .iter()
            .filter_map(|m| {
                m.github_username
                    .as_ref()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.as_str())
            })
            .collect();

        if usernames.is_empty() {
            return HashMap::new();
        }

        // Fetch all in parallel using tokio::spawn
        let handles: Vec<_> = usernames
            .iter()
            .map(|username| {
                let username = username.to_string();
                tokio::spawn(async move {
                    let github_service = GitHubService::new();
                    let result = github_service.get_creator_info(&username).await;
                    (username, result)
                })
            })
            .collect();

        let mut map: HashMap<String, GitHubCreatorInfo> = HashMap::new();
        for handle in handles {
            if let Ok((username, result)) = handle.await {
                match result {
                    Ok(info) => {
                        map.insert(username.to_lowercase(), info);
                    }
                    Err(e) => {
                        warn!("Failed to fetch GitHub info for {}: {}", username, e);
                    }
                }
            }
        }

        map
    }

    /// Get hackathon info for a single token
    /// If data is stale (>1 hour), refresh from GitHub API
    pub async fn get_hackathon_info(&self, token_id: &str) -> Option<HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());

        // Check if GitHub data needs refresh
        let github_fetched_at = controller.get_project_github_fetched_at(token_id).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let needs_refresh = match github_fetched_at {
            Some(ts) => now - ts > *HACKATHON_REFRESH_INTERVAL_SECS,
            None => true, // Never fetched
        };

        if needs_refresh {
            // Get current info first to get github_url
            if let Some(info) = controller.get_hackathon_info(token_id).await {
                info!(
                    "Hackathon data stale for {}, refreshing from GitHub",
                    token_id
                );

                let github_service = GitHubService::new();

                // Refresh project GitHub info
                if let Ok(project_info) = github_service
                    .get_project_info(&info.project.github_url)
                    .await
                {
                    if let Err(e) = controller
                        .update_project_github_info(token_id, &project_info)
                        .await
                    {
                        error!("Failed to update project GitHub info: {}", e);
                    }
                }

                // Refresh members' GitHub info
                let stale_members = controller
                    .get_members_needing_refresh(token_id, *HACKATHON_REFRESH_INTERVAL_SECS)
                    .await;

                for (member_id, github_username) in stale_members {
                    match github_service.get_creator_info(&github_username).await {
                        Ok(member_info) => {
                            if let Err(e) = controller
                                .update_member_github_info(member_id, &member_info)
                                .await
                            {
                                error!(
                                    "Failed to update member GitHub info for {}: {}",
                                    github_username, e
                                );
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to fetch GitHub info for member {}: {}",
                                github_username, e
                            );
                        }
                    }
                }

                // Return fresh data
                return controller.get_hackathon_info(token_id).await;
            }
        }

        controller.get_hackathon_info(token_id).await
    }

    /// Get hackathon infos for multiple tokens (batch)
    pub async fn get_hackathon_infos_by_token_ids(
        &self,
        token_ids: &[String],
    ) -> HashMap<String, HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());
        controller.get_hackathon_infos_by_token_ids(token_ids).await
    }
}
