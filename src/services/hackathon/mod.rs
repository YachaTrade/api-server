use std::{collections::HashMap, sync::Arc};

use tracing::{error, info};

use crate::{
    config::HACKATHON_REFRESH_INTERVAL_SECS,
    controllers::hackathon::HackathonController,
    db::postgres::PostgresDatabase,
    services::github::GitHubService,
    types::hackathon::HackathonInfo,
};

pub struct HackathonService {
    postgres: Arc<PostgresDatabase>,
}

impl HackathonService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    /// Get hackathon info for a single token
    /// If data is stale (>1 hour), refresh from GitHub API
    /// Returns None if not found or on error
    pub async fn get_hackathon_info(&self, token_id: &str) -> Option<HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());

        let (info, updated_at) = controller.get_hackathon_info_with_timestamp(token_id).await?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if now - updated_at > *HACKATHON_REFRESH_INTERVAL_SECS {
            info!(
                "Hackathon data stale for {}, refreshing from GitHub (last_update: {}s ago)",
                token_id,
                now - updated_at
            );

            if let Some(refreshed) = self
                .refresh_github_info(token_id, &info.creator.github_id, &info.project.github_url)
                .await
            {
                return Some(refreshed);
            }
        }

        Some(info)
    }

    /// Refresh GitHub info and return updated HackathonInfo
    async fn refresh_github_info(
        &self,
        token_id: &str,
        github_id: &str,
        project_github_url: &str,
    ) -> Option<HackathonInfo> {
        let github_service = GitHubService::new();

        let (creator_result, project_result) = tokio::join!(
            github_service.get_creator_info(github_id),
            github_service.get_project_info(project_github_url)
        );

        let creator_info = match creator_result {
            Ok(info) => info,
            Err(e) => {
                error!("Failed to fetch GitHub creator info for {}: {}", github_id, e);
                return None;
            }
        };

        let project_info = match project_result {
            Ok(info) => info,
            Err(e) => {
                error!(
                    "Failed to fetch GitHub project info for {}: {}",
                    project_github_url, e
                );
                return None;
            }
        };

        let controller = HackathonController::new(self.postgres.clone());
        if let Err(e) = controller
            .update_github_info_tx(token_id, github_id, &creator_info, &project_info)
            .await
        {
            error!("Failed to update hackathon GitHub info: {}", e);
            return None;
        }

        controller.get_hackathon_info(token_id).await
    }

    /// Get hackathon infos for multiple tokens (batch)
    /// Returns HashMap<token_id, HackathonInfo>
    /// Returns empty HashMap on error
    pub async fn get_hackathon_infos_by_token_ids(
        &self,
        token_ids: &[String],
    ) -> HashMap<String, HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());
        controller.get_hackathon_infos_by_token_ids(token_ids).await
    }
}
