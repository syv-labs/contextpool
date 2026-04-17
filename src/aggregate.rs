use anyhow::{Context, Result};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::scope::{self, Scope, ScopeMetadata};

/// Aggregate insights from all projects into the @interproject scope
///
/// This function:
/// 1. Walks all project directories
/// 2. Collects all indexed summaries (from projects/<id>/fetched/**/*.summary.md)
/// 3. Copies them to @interproject/fetched/<project-id>-*
/// 4. Deduplicates by content_hash
/// 5. Updates metadata
pub async fn aggregate_insights(data_dir: &Path) -> Result<AggregationStats> {
    let mut stats = AggregationStats::default();
    let projects_dir = data_dir.join("projects");

    if !projects_dir.exists() {
        return Ok(stats);
    }

    // Collect all project directories
    let mut project_dirs = Vec::new();
    for entry in fs::read_dir(&projects_dir)
        .context("Failed to read projects directory")?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && !is_special_scope_dir(&path) {
            project_dirs.push(path);
        }
    }

    // Walk each project and collect summaries
    let mut all_summaries: HashMap<String, SummaryInfo> = HashMap::new();
    for proj_dir in project_dirs {
        if let Some(proj_id) = proj_dir.file_name().and_then(|n| n.to_str()) {
            collect_summaries_from_project(&proj_dir, proj_id, &mut all_summaries)?;
        }
    }

    // Create interproject scope directory
    let interproject_dir = scope::scope_path(data_dir, &Scope::Interproject);
    fs::create_dir_all(&interproject_dir)
        .context("Failed to create @interproject directory")?;

    let fetched_dir = scope::fetched_dir(data_dir, &Scope::Interproject);
    fs::create_dir_all(&fetched_dir)
        .context("Failed to create @interproject/fetched directory")?;

    // Deduplicate by content_hash and copy to interproject
    let mut seen_hashes = HashSet::new();
    for (summary_path, summary_info) in all_summaries {
        // Apply relevance filtering
        if !crate::insight_filter::should_aggregate_insight(
            &summary_info.insight_type,
            summary_info.relevance_score,
            None,
        ) {
            stats.filtered_out += 1;
            continue;
        }

        // Skip if we've already copied a summary with this content_hash
        if seen_hashes.contains(&summary_info.content_hash) {
            stats.deduplicated += 1;
            continue;
        }
        seen_hashes.insert(summary_info.content_hash.clone());

        // Generate output filename: <project-id>-<original-name>
        let output_name = if let Some(name) = Path::new(&summary_path)
            .file_name()
            .and_then(|n| n.to_str())
        {
            format!("{}-{}", summary_info.project_id, name)
        } else {
            format!("{}-summary.md", summary_info.project_id)
        };

        let output_path = fetched_dir.join(&output_name);

        // Copy summary file
        match fs::copy(&summary_path, &output_path) {
            Ok(_) => {
                stats.copied += 1;

                // Update index entry with scope metadata
                if let Err(e) = update_index_with_scope(
                    &output_path,
                    &summary_info.project_id,
                    &summary_info.source_ide,
                ) {
                    eprintln!("Warning: Failed to update scope metadata for {}: {}", output_path.display(), e);
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to copy summary {}: {}", summary_path, e);
                stats.errors += 1;
            }
        }
    }

    // Update @interproject scope metadata
    let mut metadata = ScopeMetadata::for_interproject();
    metadata.summary_count = stats.copied;
    metadata.update_last_aggregation();

    let metadata_path = scope::scope_metadata_path(data_dir, &Scope::Interproject);
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    fs::write(&metadata_path, metadata_json)
        .context("Failed to write @interproject scope metadata")?;

    // Update root scopes.json registry
    update_scopes_registry(data_dir, &projects_dir)?;

    Ok(stats)
}

/// Statistics from aggregation run
#[derive(Debug, Default, Clone)]
pub struct AggregationStats {
    pub copied: usize,
    pub deduplicated: usize,
    pub filtered_out: usize,  // Insights excluded by relevance filter
    pub errors: usize,
}

impl std::fmt::Display for AggregationStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Aggregated {} insights ({} deduplicated, {} filtered, {} errors)",
            self.copied, self.deduplicated, self.filtered_out, self.errors
        )
    }
}

/// Information about a summary file
struct SummaryInfo {
    project_id: String,
    source_ide: String,
    content_hash: String,
    insight_type: String,
    relevance_score: f32,
}

/// Check if a directory is a special scope (like @interproject)
fn is_special_scope_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.starts_with("@"))
        .unwrap_or(false)
}

/// Walk a project directory and collect all summaries with metadata
fn collect_summaries_from_project(
    proj_dir: &Path,
    project_id: &str,
    summaries: &mut HashMap<String, SummaryInfo>,
) -> Result<()> {
    let fetched_dir = proj_dir.join("fetched");
    if !fetched_dir.exists() {
        return Ok(());
    }

    // Walk all index.json files to get metadata
    for entry in WalkDir::new(&fetched_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "index.json")
    {
        if let Ok(content) = fs::read_to_string(entry.path()) {
            if let Ok(index) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                for item in index {
                    if let Some(output_path) = item.get("output_path").and_then(|v| v.as_str()) {
                        let summary_path = fetched_dir.join(output_path);
                        if summary_path.exists() {
                            // Extract metadata from the summary file
                            let source_ide = extract_source_ide(&summary_path)
                                .unwrap_or_else(|_| "unknown".to_string());
                            let content_hash = compute_content_hash(&summary_path)
                                .unwrap_or_else(|_| {
                                    format!("hash-{}", summaries.len())
                                });
                            let (insight_type, relevance_score) = extract_insight_metadata(&summary_path)
                                .unwrap_or_else(|_| ("insight".to_string(), 0.5));

                            summaries.insert(
                                summary_path.to_string_lossy().to_string(),
                                SummaryInfo {
                                    project_id: project_id.to_string(),
                                    source_ide,
                                    content_hash,
                                    insight_type,
                                    relevance_score,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Extract the source IDE from a summary file (from metadata in the file)
fn extract_source_ide(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    // Look for "- **Source:** <ide> session" in the metadata section
    for line in content.lines() {
        if line.contains("Source:") {
            if let Some(rest) = line.split("**Source:** ").nth(1) {
                return Ok(rest.split_whitespace().next().unwrap_or("unknown").to_string());
            }
        }
    }
    Ok("unknown".to_string())
}

/// Compute a simple hash of file content (just for deduplication)
fn compute_content_hash(path: &Path) -> Result<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let content = fs::read_to_string(path)?;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(format!("{:x}", hasher.finish()))
}

/// Update index entries in interproject summaries with scope metadata
fn update_index_with_scope(
    summary_path: &Path,
    project_id: &str,
    source_ide: &str,
) -> Result<()> {
    // Find corresponding run directory
    if let Some(parent) = summary_path.parent() {
        let index_path = parent.join("index.json");
        if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            let mut entries: Vec<serde_json::Value> = serde_json::from_str(&content)?;

            // Update or add entry for this summary
            let summary_name = summary_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let mut found = false;
            for entry in &mut entries {
                if let Some(output) = entry.get("output_path").and_then(|v| v.as_str()) {
                    if output.contains(summary_name) {
                        entry["scope_id"] = json!("@interproject");
                        entry["source_project_id"] = json!(project_id);
                        entry["source_ide"] = json!(source_ide);
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                // Add new entry if not found
                entries.push(json!({
                    "source_path": format!("<aggregated from {}>", project_id),
                    "output_path": summary_name,
                    "scope_id": "@interproject",
                    "source_project_id": project_id,
                    "source_ide": source_ide,
                }));
            }

            let updated = serde_json::to_string_pretty(&entries)?;
            fs::write(&index_path, updated)?;
        }
    }
    Ok(())
}

/// Update the root scopes.json registry
fn update_scopes_registry(data_dir: &Path, projects_dir: &Path) -> Result<()> {
    let mut scopes = Vec::new();

    // Scan projects directory for all scopes
    for entry in fs::read_dir(projects_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let scope_type = if name.starts_with("@") {
                "interproject"
            } else {
                "project"
            };

            // Try to read scope.json if it exists, otherwise create stub
            let metadata_path = path.join("scope.json");
            let last_modified = if metadata_path.exists() {
                fs::metadata(&metadata_path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true).parse().ok())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            } else {
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            };

            scopes.push(json!({
                "scope_id": name,
                "scope_type": scope_type,
                "last_modified": last_modified
            }));
        }
    }

    let registry = json!({ "scopes": scopes });
    let registry_path = data_dir.join("scopes.json");
    let registry_json = serde_json::to_string_pretty(&registry)?;
    fs::write(&registry_path, registry_json)?;

    Ok(())
}

/// Extract insight type and relevance score from a summary markdown file
///
/// Parses the summary markdown to extract:
/// 1. The first insight type (from "- **<type>**" lines in the insights section)
/// 2. The relevance_score if present in the insight metadata
fn extract_insight_metadata(summary_content: &Path) -> Result<(String, f32)> {
    let content = fs::read_to_string(summary_content)?;
    let mut insight_type = String::from("insight");
    let mut relevance_score = 0.5;
    let mut in_insights_section = false;

    for line in content.lines() {
        // Track when we enter the "Extracted insights" section
        if line.contains("Extracted insights") || line.contains("extracted insights") {
            in_insights_section = true;
            continue;
        }

        // If we haven't found insights section yet, skip metadata
        if !in_insights_section {
            continue;
        }

        // Extract relevance_score from indented lines within an insight
        if line.to_lowercase().contains("relevance_score") && insight_type != "insight" {
            // Try to parse: "  - relevance_score: 0.95" or similar formats
            if let Some(score_str) = line.split(':').nth(1) {
                if let Ok(score) = score_str.trim().parse::<f32>() {
                    relevance_score = score.clamp(0.0, 1.0);
                }
            }
        }

        // Extract first insight type from "- **<type>**" pattern (not metadata)
        // Metadata lines have ":" right after "**", insight lines don't
        if line.starts_with('-') && line.contains("**") && insight_type == "insight" {
            // Look for pattern: "- **<type>** " (with space after, not colon)
            if let Some(start) = line.find("**") {
                if let Some(end) = line[start + 2..].find("**") {
                    let extracted = line[start + 2..start + 2 + end].to_lowercase();
                    let after_closing = &line[start + 2 + end + 2..];

                    // Skip if this looks like metadata (next char is colon)
                    if after_closing.starts_with(':') {
                        continue;
                    }

                    // Only accept reasonable type names (letters, dashes, underscores)
                    if extracted.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                        insight_type = extracted;
                    }
                }
            }
        }
    }

    Ok((insight_type, relevance_score))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_special_scope_dir() {
        assert!(is_special_scope_dir(Path::new("/data/@interproject")));
        assert!(!is_special_scope_dir(Path::new("/data/github.com-org-repo")));
    }

    #[test]
    fn test_aggregation_stats() {
        let stats = AggregationStats {
            copied: 10,
            deduplicated: 2,
            filtered_out: 3,
            errors: 1,
        };
        assert_eq!(stats.copied, 10);
        assert!(stats.to_string().contains("3 filtered"));
    }
}
