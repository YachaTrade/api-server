use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

use crate::utils::valid_token_id;

// ===== Request Types =====

/// Team member information for registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamMemberInput {
    /// Contact email (required)
    pub email: String,
    /// Discord handle (optional)
    pub discord: Option<String>,
    /// GitHub username (optional, enables auto-fetch of stats)
    pub github_username: Option<String>,
    /// Twitter handle (optional)
    pub twitter: Option<String>,
    /// LinkedIn URL (optional)
    pub linkedin: Option<String>,
}

/// Request body for hackathon registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterHackathonRequest {
    // === Token ===
    /// Token contract address (CA)
    pub token_id: String,

    // === Team ===
    /// Team name (required)
    pub team_name: String,
    /// Team members (1-3, at least 1 required)
    pub members: Vec<TeamMemberInput>,

    // === Project (required) ===
    /// Project name
    pub project_name: String,
    /// Project description
    pub project_description: String,
    /// Monad integration description
    pub monad_integration: String,
    /// Project GitHub URL
    pub project_github_url: String,
    /// Demo video URL
    pub demo_video_url: String,

    // === Project (optional) ===
    /// Agent Moltbook URL (optional)
    pub agent_moltbook_url: Option<String>,
}

impl RegisterHackathonRequest {
    pub fn validate(&self) -> Result<(), String> {
        // Token validation
        if valid_token_id(&self.token_id).is_none() {
            return Err("Invalid token_id format".to_string());
        }

        // Team validation
        if self.team_name.trim().is_empty() {
            return Err("team_name is required".to_string());
        }
        if self.members.is_empty() {
            return Err("At least 1 team member is required".to_string());
        }
        if self.members.len() > 3 {
            return Err("Maximum 3 team members allowed".to_string());
        }

        // Validate each member and check for duplicates
        let mut seen_emails: HashSet<String> = HashSet::new();
        let mut seen_github_usernames: HashSet<String> = HashSet::new();

        for (i, member) in self.members.iter().enumerate() {
            // Email validation: required, must contain '@' and '.'
            let email = member.email.trim().to_lowercase();
            if email.is_empty() {
                return Err(format!("Email is required for member {}", i + 1));
            }
            if !email.contains('@') || !email.contains('.') {
                return Err(format!(
                    "Invalid email format for member {}: must contain '@' and '.'",
                    i + 1
                ));
            }

            // Check for duplicate email
            if seen_emails.contains(&email) {
                return Err(format!("Duplicate email '{}' found in members", email));
            }
            seen_emails.insert(email);

            // Check for duplicate github_username (if provided)
            if let Some(ref gh_username) = member.github_username {
                let gh_username_lower = gh_username.trim().to_lowercase();
                if !gh_username_lower.is_empty() {
                    if seen_github_usernames.contains(&gh_username_lower) {
                        return Err(format!(
                            "Duplicate github_username '{}' found in members",
                            gh_username
                        ));
                    }
                    seen_github_usernames.insert(gh_username_lower);
                }
            }

            // LinkedIn URL validation (if provided)
            if let Some(ref linkedin) = member.linkedin {
                if !linkedin.trim().is_empty() && !linkedin.starts_with("https://") {
                    return Err(format!(
                        "LinkedIn URL for member {} must start with https://",
                        i + 1
                    ));
                }
            }
        }

        // Project validation
        if self.project_name.trim().is_empty() {
            return Err("project_name is required".to_string());
        }
        if self.project_description.trim().is_empty() {
            return Err("project_description is required".to_string());
        }
        if self.monad_integration.trim().is_empty() {
            return Err("monad_integration is required".to_string());
        }
        if !self.project_github_url.starts_with("https://github.com/") {
            return Err("project_github_url must start with https://github.com/".to_string());
        }
        if self.demo_video_url.trim().is_empty() {
            return Err("demo_video_url is required".to_string());
        }
        // Demo video URL validation
        if !self.demo_video_url.starts_with("https://") {
            return Err("demo_video_url must start with https://".to_string());
        }

        // Optional URL validations
        if let Some(ref url) = self.agent_moltbook_url {
            if !url.trim().is_empty() && !url.starts_with("https://") {
                return Err("agent_moltbook_url must start with https://".to_string());
            }
        }

        Ok(())
    }
}

// ===== Response Types =====

/// Result for each item in registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterHackathonItemResult {
    pub token_id: String,
    /// "registered", "skipped" (already exists), or "error"
    pub status: String,
    /// Team ID if registered
    pub team_id: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Indicates if GitHub stats fetch was deferred
    pub github_fetch_pending: bool,
}

/// Batch response for hackathon registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterHackathonBatchResponse {
    /// Total items processed
    pub total: usize,
    /// Successfully registered count
    pub registered: usize,
    /// Skipped (already exists) count
    pub skipped: usize,
    /// Failed count
    pub failed: usize,
    /// Individual results
    pub results: Vec<RegisterHackathonItemResult>,
}

/// Response for hackathon token list
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonTokenListResponse {
    pub token_ids: Vec<String>,
}

/// Team member info in API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonTeamMemberInfo {
    pub email: String,
    pub discord: Option<String>,
    pub twitter: Option<String>,
    pub linkedin: Option<String>,
    /// GitHub info (if github_username was provided)
    pub github: Option<HackathonMemberGitHubInfo>,
}

/// GitHub info for a team member
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonMemberGitHubInfo {
    pub username: String,
    pub image_uri: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub follower_count: Option<i32>,
    pub following_count: Option<i32>,
    pub repo_count: Option<i32>,
    pub star_count: Option<i32>,
    pub bio: Option<String>,
    /// True if stats haven't been fetched yet
    pub fetch_pending: bool,
}

/// Team info in API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonTeamInfo {
    pub id: String,
    pub name: String,
    pub members: Vec<HackathonTeamMemberInfo>,
}

/// Project info in API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonProjectInfo {
    pub name: String,
    pub description: String,
    pub monad_integration: String,
    pub github_url: String,
    pub demo_video_url: String,
    pub agent_moltbook_url: Option<String>,
    // GitHub repo stats
    pub github_star_count: i32,
    pub github_fork_count: i32,
    pub github_description: Option<String>,
    pub github_topics: Option<Vec<String>>,
    pub github_language: Option<String>,
}

/// Hackathon info to be embedded in TokenInfo
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonInfo {
    pub team: HackathonTeamInfo,
    pub project: HackathonProjectInfo,
}
