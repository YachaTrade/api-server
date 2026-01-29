use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::utils::valid_evm_address;

/// Request body for hackathon registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterHackathonRequest {
    /// Creator's GitHub ID (username)
    pub github_id: String,
    /// Twitter handle (required)
    pub twitter: String,
    /// Discord handle (optional)
    pub discord: Option<String>,
    /// Telegram handle (optional)
    pub telegram: Option<String>,
    /// LinkedIn URL (optional)
    pub linkedin: Option<String>,
    /// Project GitHub URL (e.g., https://github.com/owner/repo)
    pub project_github_url: String,
    /// Project name
    pub project_name: String,
    /// Project description
    pub project_description: String,
    /// Keywords (comma-separated)
    pub keywords: String,
    /// Screenshot URI (uploaded image URL)
    pub screenshot_uri: String,
    /// Website URL (optional)
    pub website: Option<String>,
    /// YouTube URL (optional)
    pub youtube: Option<String>,
    /// Token contract address
    pub token_id: String,
    /// Developer wallet address
    pub account_id: String,
}

impl RegisterHackathonRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.github_id.is_empty() {
            return Err("github_id is required".to_string());
        }
        if self.twitter.is_empty() {
            return Err("twitter is required".to_string());
        }
        if self.project_github_url.is_empty()
            || !self.project_github_url.starts_with("https://github.com/")
        {
            return Err(
                "Invalid project_github_url - must start with https://github.com/".to_string(),
            );
        }
        if self.project_name.is_empty() {
            return Err("project_name is required".to_string());
        }
        if self.project_description.is_empty() {
            return Err("project_description is required".to_string());
        }
        if self.keywords.is_empty() {
            return Err("keywords is required".to_string());
        }
        if self.screenshot_uri.is_empty() {
            return Err("screenshot_uri is required".to_string());
        }
        if !valid_evm_address(&self.token_id) {
            return Err("Invalid token_id format".to_string());
        }
        if !valid_evm_address(&self.account_id) {
            return Err("Invalid account_id format".to_string());
        }
        Ok(())
    }
}

/// Response for hackathon registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterHackathonResponse {
    pub success: bool,
    pub token_id: String,
}

/// Response for hackathon token list
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonTokenListResponse {
    pub token_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonCreatorInfo {
    pub github_id: String,
    pub image_uri: Option<String>,
    pub name: Option<String>,
    pub github_url: Option<String>,
    pub follower_count: i32,
    pub following_count: i32,
    pub repo_count: i32,
    pub star_count: i32,
    pub bio: Option<String>,
    pub twitter: String,
    pub discord: Option<String>,
    pub telegram: Option<String>,
    pub linkedin: Option<String>,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonProjectInfo {
    pub github_url: String,
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub screenshot_uri: String,
    pub website: Option<String>,
    pub youtube: Option<String>,
    pub star_count: i32,
    pub fork_count: i32,
    pub topics: Option<Vec<String>>,
    pub language: Option<String>,
}

/// Hackathon info to be embedded in TokenInfo
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HackathonInfo {
    pub creator: HackathonCreatorInfo,
    pub project: HackathonProjectInfo,
}
