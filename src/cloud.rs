//! Client for the ContextPool cloud API (`cxp-server`).
//!
//! Used by `cxp auth`, `cxp push`, `cxp pull`, and `cxp team` subcommands.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "contextpool-team";
const KEYRING_USER: &str = "api-key";
const ENV_KEY: &str = "CXP_API_KEY";
const DEFAULT_API_URL: &str = "https://api.contextpool.dev";

// ── API key storage ─────────────────────────────────────────────────────────

pub fn load_team_api_key() -> Option<String> {
    // Env var takes precedence (CI, containers, etc.)
    if let Ok(v) = std::env::var(ENV_KEY) {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }

    // System keychain
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if let Ok(v) = entry.get_password() {
            let t = v.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }

    // File-based fallback
    if let Ok(v) = std::fs::read_to_string(key_file_path()) {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }

    None
}

pub fn save_team_api_key(key: &str) -> Result<()> {
    // Try system keychain first
    let keychain_ok = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .and_then(|entry| entry.set_password(key))
        .is_ok();

    // Always write file-based fallback so the key is never lost
    let fallback = key_file_path();
    if let Some(parent) = fallback.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&fallback, key)
        .with_context(|| format!("Failed to write key to {}", fallback.display()))?;

    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o600));
    }

    if !keychain_ok {
        eprintln!(
            "Note: System keychain unavailable, key saved to {}",
            fallback.display()
        );
    }

    Ok(())
}

pub fn delete_team_api_key() -> Result<()> {
    // Remove from keychain
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => eprintln!("Warning: could not clear keychain entry: {e}"),
        }
    }
    // Remove file fallback
    let path = key_file_path();
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn key_file_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
        .join("contextpool")
        .join("team-key")
}

fn api_url() -> String {
    std::env::var("CXP_API_URL")
        .unwrap_or_else(|_| DEFAULT_API_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn require_api_key() -> Result<String> {
    load_team_api_key().ok_or_else(|| {
        anyhow::anyhow!(
            "Not authenticated. Run `cxp auth <team-key>` first, or set CXP_API_KEY env var."
        )
    })
}

// ── API types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
pub struct TeamInfo {
    pub team_id: String,
    pub name: String,
    pub plan: String,
    pub usage: TeamUsage,
}

#[derive(Deserialize, Debug)]
pub struct TeamUsage {
    pub insights: u64,
    pub projects: u64,
    pub members: u64,
    pub limits: TeamLimits,
}

#[derive(Deserialize, Debug)]
pub struct TeamLimits {
    pub insights: u64,
    pub projects: u64,
    pub members: u64,
}

#[derive(Serialize)]
pub struct PushRequest {
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ide: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributor: Option<String>,
    pub insights: Vec<PushInsight>,
}

#[derive(Serialize)]
pub struct PushInsight {
    #[serde(rename = "type")]
    pub r#type: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PushResponse {
    pub inserted: u64,
    pub skipped: u64,
    pub total: u64,
}

#[derive(Deserialize, Debug)]
pub struct PullResponse {
    pub insights: Vec<RemoteInsight>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RemoteInsight {
    pub id: String,
    pub project_id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub file: Option<String>,
    pub source_ide: String,
    pub contributor: String,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct ProjectsResponse {
    pub projects: Vec<RemoteProject>,
}

#[derive(Deserialize, Debug)]
pub struct RemoteProject {
    pub project_id: String,
    pub display_name: String,
    pub insight_count: u64,
    pub created_at: String,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

// ── API calls ───────────────────────────────────────────────────────────────

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

async fn check_response(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if let Ok(err) = serde_json::from_str::<ApiErrorBody>(&body) {
        anyhow::bail!("{} — {}", status, err.error.message);
    }
    anyhow::bail!("{} — {}", status, body);
}

pub async fn get_team_info() -> Result<TeamInfo> {
    let key = require_api_key()?;
    get_team_info_with_key(&key).await
}

/// Verify a key directly without loading from storage — used by `cxp auth`.
pub async fn get_team_info_with_key(key: &str) -> Result<TeamInfo> {
    let url = format!("{}/api/teams/me", api_url());
    let resp = client()
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .context("Failed to reach ContextPool API")?;
    let resp = check_response(resp).await?;
    resp.json().await.context("Invalid API response")
}

pub async fn push_insights(req: &PushRequest) -> Result<PushResponse> {
    let key = require_api_key()?;
    let url = format!("{}/api/insights", api_url());
    let resp = client()
        .post(&url)
        .bearer_auth(&key)
        .json(req)
        .send()
        .await
        .context("Failed to reach ContextPool API")?;
    let resp = check_response(resp).await?;
    resp.json().await.context("Invalid API response")
}

pub async fn pull_insights(project_id: &str) -> Result<PullResponse> {
    let key = require_api_key()?;
    let url = format!("{}/api/insights?project_id={}", api_url(), project_id);
    let resp = client()
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .context("Failed to reach ContextPool API")?;
    let resp = check_response(resp).await?;
    resp.json().await.context("Invalid API response")
}

pub async fn list_team_projects() -> Result<ProjectsResponse> {
    let key = require_api_key()?;
    let url = format!("{}/api/projects", api_url());
    let resp = client()
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .context("Failed to reach ContextPool API")?;
    let resp = check_response(resp).await?;
    resp.json().await.context("Invalid API response")
}
