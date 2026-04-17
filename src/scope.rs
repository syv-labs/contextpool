use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::Result;

/// Represents a memory scope - either project-specific or inter-project
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scope {
    /// Project-specific scope, keyed by project ID
    Project(String),
    /// Inter-project scope - aggregated insights from all projects
    Interproject,
}

impl Scope {
    pub fn is_project(&self) -> bool {
        matches!(self, Scope::Project(_))
    }

    pub fn is_interproject(&self) -> bool {
        matches!(self, Scope::Interproject)
    }

    pub fn project_id(&self) -> Option<&str> {
        match self {
            Scope::Project(id) => Some(id),
            Scope::Interproject => None,
        }
    }

    /// Convert to a string identifier for storage/serialization
    pub fn as_id(&self) -> String {
        match self {
            Scope::Project(id) => id.clone(),
            Scope::Interproject => "@interproject".to_string(),
        }
    }

    /// Parse from a string identifier
    pub fn from_id(id: &str) -> Self {
        if id == "@interproject" {
            Scope::Interproject
        } else {
            Scope::Project(id.to_string())
        }
    }

    /// Get the scope type for metadata ("project" or "interproject")
    pub fn scope_type(&self) -> &str {
        match self {
            Scope::Project(_) => "project",
            Scope::Interproject => "interproject",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_id())
    }
}

/// Get the filesystem path for a scope within the data directory
pub fn scope_path(data_dir: &Path, scope: &Scope) -> PathBuf {
    match scope {
        Scope::Project(id) => data_dir.join("projects").join(id),
        Scope::Interproject => data_dir.join("projects").join("@interproject"),
    }
}

/// Get the path to the fetched summaries directory for a scope
pub fn fetched_dir(data_dir: &Path, scope: &Scope) -> PathBuf {
    scope_path(data_dir, scope).join("fetched")
}

/// Get the path to the scope metadata file
pub fn scope_metadata_path(data_dir: &Path, scope: &Scope) -> PathBuf {
    scope_path(data_dir, scope).join("scope.json")
}

/// Metadata about a scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeMetadata {
    pub scope_id: String,
    pub scope_type: String, // "project" or "interproject"
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_projects: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_aggregation: Option<String>,
    pub summary_count: usize,
}

impl ScopeMetadata {
    pub fn for_project(project_id: String) -> Self {
        Self {
            scope_id: project_id,
            scope_type: "project".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_projects: None,
            last_aggregation: None,
            summary_count: 0,
        }
    }

    pub fn for_interproject() -> Self {
        Self {
            scope_id: "@interproject".to_string(),
            scope_type: "interproject".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_projects: Some(vec![]),
            last_aggregation: None,
            summary_count: 0,
        }
    }

    pub fn update_last_aggregation(&mut self) {
        self.last_aggregation = Some(chrono::Utc::now().to_rfc3339());
    }
}

/// CLI command: list all scopes
pub async fn cmd_list_scopes() -> Result<()> {
    let data_dir = crate::paths::default_out_dir();
    let scopes_registry_path = data_dir.join("scopes.json");

    if !scopes_registry_path.exists() {
        println!("No scopes found. Run 'cxp fetch' to initialize.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&scopes_registry_path)?;
    let registry: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(scopes) = registry["scopes"].as_array() {
        if scopes.is_empty() {
            println!("No scopes found. Run 'cxp fetch' to initialize.");
        } else {
            println!("Available scopes:\n");
            for scope in scopes {
                let id = scope["scope_id"].as_str().unwrap_or("unknown");
                let scope_type = scope["scope_type"].as_str().unwrap_or("unknown");
                let modified = scope["last_modified"].as_str().unwrap_or("unknown");

                if scope_type == "interproject" {
                    println!("  {} (inter-project, aggregated)  \n    last updated: {}", id, modified);
                } else {
                    println!("  {}  \n    type: {}  \n    last modified: {}", id, scope_type, modified);
                }
            }
        }
    }

    Ok(())
}

/// CLI command: show scope info
pub async fn cmd_scope_info(scope_id: &str) -> Result<()> {
    let data_dir = crate::paths::default_out_dir();
    let scope = Scope::from_id(scope_id);
    let metadata_path = scope_metadata_path(&data_dir, &scope);

    if !metadata_path.exists() {
        println!("Scope '{}' not found.", scope_id);
        return Ok(());
    }

    let content = std::fs::read_to_string(&metadata_path)?;
    let metadata: ScopeMetadata = serde_json::from_str(&content)?;

    println!("Scope: {}", metadata.scope_id);
    println!("Type: {}", metadata.scope_type);
    println!("Created: {}", metadata.created_at);
    println!("Summaries: {}", metadata.summary_count);

    if let Some(last_agg) = &metadata.last_aggregation {
        println!("Last aggregation: {}", last_agg);
    }

    if let Some(projects) = &metadata.parent_projects {
        if !projects.is_empty() {
            println!("Member projects: {}", projects.join(", "));
        }
    }

    Ok(())
}

/// CLI command: manually trigger aggregation
pub async fn cmd_aggregate(args: crate::cli::AggregateArgs) -> Result<()> {
    let data_dir = args.data_dir.unwrap_or_else(crate::paths::default_out_dir);

    println!("Aggregating inter-project insights...");

    match crate::aggregate::aggregate_insights(&data_dir).await {
        Ok(stats) => {
            println!("✓ Aggregation complete:");
            println!("  {} insights copied", stats.copied);
            if stats.filtered_out > 0 {
                println!("  {} filtered (low relevance)", stats.filtered_out);
            }
            if stats.deduplicated > 0 {
                println!("  {} deduplicated", stats.deduplicated);
            }
            if stats.errors > 0 {
                println!("  {} errors", stats.errors);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ Aggregation failed: {}", e);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_as_id() {
        let proj = Scope::Project("github.com-org-repo".to_string());
        assert_eq!(proj.as_id(), "github.com-org-repo");
        assert_eq!(Scope::Interproject.as_id(), "@interproject");
    }

    #[test]
    fn test_scope_from_id() {
        let proj = Scope::from_id("github.com-org-repo");
        assert!(matches!(proj, Scope::Project(_)));

        let inter = Scope::from_id("@interproject");
        assert!(matches!(inter, Scope::Interproject));
    }

    #[test]
    fn test_scope_type() {
        assert_eq!(Scope::Project("test".to_string()).scope_type(), "project");
        assert_eq!(Scope::Interproject.scope_type(), "interproject");
    }

    #[test]
    fn test_scope_paths() {
        let data_dir = Path::new("/data");
        let proj = Scope::Project("github.com-org-repo".to_string());
        let inter = Scope::Interproject;

        assert_eq!(
            scope_path(&data_dir, &proj),
            PathBuf::from("/data/projects/github.com-org-repo")
        );
        assert_eq!(
            scope_path(&data_dir, &inter),
            PathBuf::from("/data/projects/@interproject")
        );
    }
}
