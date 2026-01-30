use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::config::GITHUB_TOKEN;

/// GitHub API 응답 - 사용자 정보
#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub avatar_url: Option<String>,
    pub html_url: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub followers: i32,
    pub following: i32,
    pub public_repos: i32,
}

/// GitHub API 응답 - 레포지토리 정보
#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub stargazers_count: i32,
    pub forks_count: i32,
    pub topics: Option<Vec<String>>,
    pub language: Option<String>,
}

/// GitHub API 응답 - 레포지토리 목록 (스타 계산용)
#[derive(Debug, Deserialize)]
struct GitHubRepoMinimal {
    stargazers_count: i32,
}

/// 개발자 정보 (API에서 가져온 것)
#[derive(Debug, Clone)]
pub struct GitHubCreatorInfo {
    pub image_uri: Option<String>,
    pub name: Option<String>,
    pub github_url: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub repo_count: i32,
    pub star_count: i32,
    pub bio: Option<String>,
}

/// 프로젝트 정보 (API에서 가져온 것)
#[derive(Debug, Clone)]
pub struct GitHubProjectInfo {
    pub star_count: i32,
    pub fork_count: i32,
    pub topics: Option<Vec<String>>,
    pub language: Option<String>,
}

pub struct GitHubService {
    client: Client,
}

impl GitHubService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// GitHub 사용자 정보 조회
    pub async fn get_creator_info(&self, github_id: &str) -> Result<GitHubCreatorInfo> {
        let url = format!("https://api.github.com/users/{}", github_id);

        // Fetch user info and total stars in parallel
        let user_future = async {
            self.client
                .get(&url)
                .header("User-Agent", "nads-pump-api")
                .header("Accept", "application/vnd.github.v3+json")
                .header("Authorization", format!("Bearer {}", &*GITHUB_TOKEN))
                .send()
                .await?
                .error_for_status()
                .map_err(|e| anyhow!("GitHub API error: {}", e))?
                .json::<GitHubUser>()
                .await
                .map_err(|e| anyhow!("Failed to parse GitHub user: {}", e))
        };

        let (user_result, star_count) = tokio::join!(
            user_future,
            self.get_total_stars(github_id)
        );

        let user = user_result?;
        let star_count = star_count.unwrap_or(0);

        Ok(GitHubCreatorInfo {
            image_uri: user.avatar_url,
            name: user.name,
            github_url: user.html_url,
            follower_count: user.followers,
            following_count: user.following,
            repo_count: user.public_repos,
            star_count,
            bio: user.bio,
        })
    }

    /// 사용자의 모든 레포에서 총 스타 수 계산
    async fn get_total_stars(&self, github_id: &str) -> Result<i32> {
        let url = format!(
            "https://api.github.com/users/{}/repos?per_page=100&sort=updated",
            github_id
        );

        let repos: Vec<GitHubRepoMinimal> = self
            .client
            .get(&url)
            .header("User-Agent", "nads-pump-api")
            .header("Accept", "application/vnd.github.v3+json")
            .header("Authorization", format!("Bearer {}", &*GITHUB_TOKEN))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let total = repos.iter().map(|r| r.stargazers_count).sum();
        Ok(total)
    }

    /// GitHub 레포지토리 정보 조회 (github_url에서 owner/repo 파싱)
    pub async fn get_project_info(&self, github_url: &str) -> Result<GitHubProjectInfo> {
        let (owner, repo) = self.parse_github_url(github_url)?;
        let url = format!("https://api.github.com/repos/{}/{}", owner, repo);

        let repo_info: GitHubRepo = self
            .client
            .get(&url)
            .header("User-Agent", "nads-pump-api")
            .header("Accept", "application/vnd.github.v3+json")
            .header("Authorization", format!("Bearer {}", &*GITHUB_TOKEN))
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow!("GitHub API error: {}", e))?
            .json()
            .await?;

        Ok(GitHubProjectInfo {
            star_count: repo_info.stargazers_count,
            fork_count: repo_info.forks_count,
            topics: repo_info.topics,
            language: repo_info.language,
        })
    }

    /// GitHub URL에서 owner/repo 파싱
    /// 예: https://github.com/owner/repo -> (owner, repo)
    fn parse_github_url(&self, url: &str) -> Result<(String, String)> {
        let url = url
            .trim_end_matches('/')
            .replace("https://github.com/", "")
            .replace("http://github.com/", "");

        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            Ok((parts[0].to_string(), parts[1].to_string()))
        } else {
            Err(anyhow!("Invalid GitHub URL format: {}", url))
        }
    }
}

impl Default for GitHubService {
    fn default() -> Self {
        Self::new()
    }
}
