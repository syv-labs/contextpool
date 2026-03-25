use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParamInner, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    Error as McpError, RoleServer, ServerHandler, ServiceExt,
};
use walkdir::WalkDir;

use crate::{
    embedded_agent::{ContextItem, EmbeddedAgentOptions},
    paths::default_out_dir,
    project::{project_dir, project_id_from_path},
};

#[derive(Clone)]
pub struct ContextPoolServer {
    data_dir: PathBuf,
}

impl ContextPoolServer {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    async fn fetch_project_context_impl(&self, project_path: Option<String>) -> String {
        let path = resolve_project_path(project_path);

        let local_base = path.join("ContextPool");
        let project_id = project_id_from_path(&path);
        let proj_dir = project_dir(&local_base, &project_id);

        if let Err(e) = std::fs::create_dir_all(&proj_dir) {
            return format!("Failed to create ContextPool dir: {e}");
        }

        // Write project.json once
        let meta_path = proj_dir.join("project.json");
        if !meta_path.exists() {
            let meta = serde_json::json!({
                "project_id": &project_id,
                "root_path": path.to_string_lossy(),
                "created_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            });
            let _ = std::fs::write(
                &meta_path,
                serde_json::to_string_pretty(&meta).unwrap_or_default(),
            );
        }

        // Require API key to be pre-configured (keychain / env) — no interactive prompt in MCP.
        let backend = match crate::credentials::load_api_backend() {
            Some(b) => b,
            None => {
                return "No API key configured. \
                    Set ANTHROPIC_API_KEY (preferred when running inside Claude Code / Cursor) \
                    or NVIDIA_API_KEY, or run `cxp export cursor` once from a terminal \
                    to save the NVIDIA key to the system keychain."
                    .to_string()
            }
        };

        // Collect transcript files not yet indexed
        let already_indexed = collect_indexed_source_paths(&proj_dir);
        let new_files = discover_new_transcripts(&path, &project_id, &already_indexed);

        if new_files.is_empty() {
            return format_context_index(&proj_dir, 0);
        }

        // Summarize new transcripts
        let opts = EmbeddedAgentOptions::from_env(backend);
        let run_id = Utc::now()
            .to_rfc3339_opts(SecondsFormat::Secs, true)
            .replace(':', "-");
        let run_dir = proj_dir.join("fetched").join(&run_id);
        if let Err(e) = std::fs::create_dir_all(&run_dir) {
            return format!("Failed to create run dir: {e}");
        }

        let mut index_entries: Vec<serde_json::Value> = Vec::new();
        let mut imported = 0;

        for transcript_path in &new_files {
            let Ok(raw) = std::fs::read_to_string(transcript_path) else {
                continue;
            };
            let extracted = crate::transcript::extract_text_from_jsonl(&raw);

            let (items, _) =
                match crate::embedded_agent::generate_context_items(&extracted, &[], "", &opts)
                    .await
                {
                    Ok(r) => r,
                    Err(_) => {
                        // Mark as indexed so we don't retry indefinitely
                        index_entries.push(serde_json::json!({
                            "source_path": transcript_path.to_string_lossy(),
                            "output_path": null,
                            "chars_in": extracted.len(),
                        }));
                        continue;
                    }
                };

            // Always mark as indexed regardless of whether we got insights
            if items.is_empty() {
                index_entries.push(serde_json::json!({
                    "source_path": transcript_path.to_string_lossy(),
                    "output_path": null,
                    "chars_in": extracted.len(),
                }));
                continue;
            }

            let safe_name = transcript_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("session")
                .to_string();
            let out_file = run_dir.join(format!("{safe_name}.summary.md"));

            let summary_md = items_to_markdown(&items);
            let _ = std::fs::write(
                &out_file,
                format!(
                    "# Summary\n\n{}\n\n## Source\n- `{}`\n",
                    summary_md,
                    transcript_path.display()
                ),
            );

            index_entries.push(serde_json::json!({
                "source_path": transcript_path.to_string_lossy(),
                "output_path": out_file.to_string_lossy(),
                "chars_in": extracted.len(),
            }));
            imported += 1;
        }

        let index_path = run_dir.join("index.json");
        let _ = std::fs::write(
            &index_path,
            serde_json::to_string_pretty(&index_entries).unwrap_or_default(),
        );

        format_context_index(&proj_dir, imported)
    }

    fn get_project_context_impl(
        &self,
        project_path: Option<String>,
        ids: Option<Vec<String>>,
    ) -> String {
        let path = resolve_project_path(project_path);
        let project_id = project_id_from_path(&path);

        // Local ContextPool takes priority over the global data dir
        let local_proj_dir = project_dir(&path.join("ContextPool"), &project_id);
        let proj_dir = if local_proj_dir.exists() {
            local_proj_dir
        } else {
            project_dir(&self.data_dir, &project_id)
        };

        match ids {
            Some(ids) if !ids.is_empty() => collect_summaries_by_ids(&proj_dir, &ids),
            _ => collect_all_summaries(&proj_dir),
        }
    }

    fn search_context_impl(&self, query: &str, project_path: Option<String>) -> String {
        let root = match project_path {
            Some(p) => {
                let path = PathBuf::from(&p);
                let id = project_id_from_path(&path);
                let local = project_dir(&path.join("ContextPool"), &id);
                if local.exists() {
                    local
                } else {
                    project_dir(&self.data_dir, &id)
                }
            }
            None => self.data_dir.join("projects"),
        };
        search_summaries(&root, query)
    }

    fn list_projects_impl(&self) -> String {
        list_projects(&self.data_dir)
    }
}

impl ServerHandler for ContextPoolServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "contextpool".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "ContextPool gives you persistent memory across sessions. Follow these rules:\n\n\
                 If this is the first message in a new conversation for this project, call \
                 fetch_project_context once to index any new transcripts and load the summary index. \
                 Then call get_project_context with relevant ids to load insights before starting work.\n\n\
                 When the user references a past conversation, asks you to remember something discussed before, \
                 or says things like \"we talked about this\", \"remember when we...\", \"in our last session\", \
                 first try search_context or get_project_context with relevant keywords to find the discussion. \
                 Only call fetch_project_context if the search returns no results — fetching re-indexes \
                 transcripts and will pick up sessions that haven't been summarized yet.\n\n\
                 When debugging a bug, encountering an error, or making an architectural decision, \
                 call search_context with relevant keywords (error messages, component names, library names) \
                 to check if the issue was addressed in a prior session.\n\n\
                 Use list_context_projects to see all projects with stored context when the user \
                 works across multiple repositories."
                    .into(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParamInner>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let schema = |json: serde_json::Value| {
            Arc::new(
                serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(json)
                    .expect("static schema is valid"),
            )
        };

        Ok(ListToolsResult {
            next_cursor: None,
            tools: vec![
                Tool {
                    name: "fetch_project_context".into(),
                    description: "Discover and summarize Cursor and Claude Code transcripts for \
                         this project. Stores summaries locally in <project>/ContextPool/. \
                         Returns a compact index (ids, insight titles, tags) of all available \
                         summaries. Call this first, then call get_project_context with the \
                         relevant ids to load the full details."
                        .into(),
                    input_schema: schema(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "project_path": {
                                "type": "string",
                                "description": "Absolute path to the project root. Defaults to cwd."
                            }
                        }
                    })),
                },
                Tool {
                    name: "get_project_context".into(),
                    description: "Load full content of stored context summaries into memory. \
                         Pass ids from fetch_project_context to load specific summaries, or omit \
                         ids to load everything. Returns the full markdown with insights, bug \
                         fixes, and decisions."
                        .into(),
                    input_schema: schema(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "project_path": {
                                "type": "string",
                                "description": "Absolute path to the project root. Defaults to cwd."
                            },
                            "ids": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Summary ids from fetch_project_context to load. Omit to load all."
                            }
                        }
                    })),
                },
                Tool {
                    name: "search_context".into(),
                    description: "Search stored context summaries for a keyword or phrase. \
                         Useful for finding whether a bug was seen before or how a past \
                         architectural decision was made."
                        .into(),
                    input_schema: schema(serde_json::json!({
                        "type": "object",
                        "required": ["query"],
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "Keyword or phrase to search for across summaries."
                            },
                            "project_path": {
                                "type": "string",
                                "description": "Limit search to this project. Omit to search all projects."
                            }
                        }
                    })),
                },
                Tool {
                    name: "list_context_projects".into(),
                    description: "List all projects that have stored context, with summary counts."
                        .into(),
                    input_schema: schema(serde_json::json!({
                        "type": "object",
                        "properties": {}
                    })),
                },
            ],
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = request.arguments.unwrap_or_default();

        let text = match request.name.as_ref() {
            "fetch_project_context" => {
                let project_path = args
                    .get("project_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.fetch_project_context_impl(project_path).await
            }
            "get_project_context" => {
                let project_path = args
                    .get("project_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let ids = args.get("ids").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(String::from)
                        .collect::<Vec<_>>()
                });
                self.get_project_context_impl(project_path, ids)
            }
            "search_context" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let project_path = args
                    .get("project_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.search_context_impl(query, project_path)
            }
            "list_context_projects" => self.list_projects_impl(),
            name => {
                return Err(McpError::invalid_params(
                    format!("Unknown tool: {name}"),
                    None,
                ))
            }
        };

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn resolve_project_path(project_path: Option<String>) -> PathBuf {
    project_path
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Walk all `index.json` files under `proj_dir` and collect every source_path
/// that has already been processed, so we don't re-summarize on future calls.
fn collect_indexed_source_paths(proj_dir: &Path) -> HashSet<String> {
    let mut paths = HashSet::new();
    for entry in WalkDir::new(proj_dir).follow_links(false) {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if e.file_name().to_str() != Some("index.json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&content) else {
            continue;
        };
        for item in arr {
            if let Some(src) = item["source_path"].as_str() {
                paths.insert(src.to_string());
            }
        }
    }
    paths
}

/// Find Cursor and Claude Code transcript files for `project_id` / `project_path`
/// that aren't in `already_indexed`.
fn discover_new_transcripts(
    project_path: &Path,
    project_id: &str,
    already_indexed: &HashSet<String>,
) -> Vec<PathBuf> {
    let mut new_files: Vec<PathBuf> = Vec::new();

    // Cursor: ~/.cursor/projects/<project_id>/agent-transcripts/
    if let Some(cursor_dir) = crate::paths::default_cursor_dir() {
        let root = cursor_dir
            .join("projects")
            .join(project_id)
            .join("agent-transcripts");
        collect_jsonl_files(&root, already_indexed, &mut new_files);
    }

    // Claude Code: ~/.claude/projects/<cc_dir_name>/
    if let Some(claude_dir) = crate::paths::default_claude_code_dir() {
        let cc_dir_name =
            crate::export::claude_code::claude_code_project_dir_name(project_path);
        let root = claude_dir.join("projects").join(&cc_dir_name);
        collect_jsonl_files(&root, already_indexed, &mut new_files);
    }

    new_files
}

fn collect_jsonl_files(
    root: &Path,
    already_indexed: &HashSet<String>,
    out: &mut Vec<PathBuf>,
) {
    if !root.exists() {
        return;
    }
    for entry in WalkDir::new(root).follow_links(false) {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().extension() != Some(OsStr::new("jsonl")) {
            continue;
        }
        if !already_indexed.contains(&e.path().to_string_lossy().to_string()) {
            out.push(e.into_path());
        }
    }
}

/// Convert `Vec<ContextItem>` to the same markdown format the CLI uses.
fn items_to_markdown(items: &[ContextItem]) -> String {
    let mut out = String::from("## Extracted insights\n\n");
    for it in items {
        let ty = if it.r#type.trim().is_empty() {
            "insight"
        } else {
            it.r#type.trim()
        };
        let title = it.title.trim();
        let summary = it.summary.trim();

        if title.is_empty() && summary.is_empty() {
            continue;
        }
        if title.is_empty() {
            out.push_str(&format!("- **{}**: {}\n", ty, summary));
        } else if summary.is_empty() {
            out.push_str(&format!("- **{}** {}.\n", ty, title));
        } else {
            out.push_str(&format!("- **{}** {} — {}\n", ty, title, summary));
        }
        if let Some(f) = it.file.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            out.push_str(&format!("  - file: `{}`\n", f));
        }
        let tags = it
            .tags
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !tags.is_empty() {
            out.push_str(&format!("  - tags: {}\n", tags));
        }
    }
    out.trim().to_string()
}

/// Build the compact index string that `fetch_project_context` returns.
fn format_context_index(proj_dir: &Path, newly_imported: usize) -> String {
    let mut entries: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();

    for entry in WalkDir::new(proj_dir).follow_links(false).sort_by_file_name() {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if !e.file_name().to_str().unwrap_or("").ends_with(".summary.md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let id = e
            .path()
            .strip_prefix(proj_dir)
            .unwrap_or(e.path())
            .to_string_lossy()
            .to_string();
        let (titles, tags) = parse_insight_metadata(&content);
        entries.push((id, titles, tags));
    }

    if entries.is_empty() {
        return "No transcripts found for this project in Cursor or Claude Code.".to_string();
    }

    let header = if newly_imported > 0 {
        format!("Indexed {} new session(s). ", newly_imported)
    } else {
        "All sessions already indexed. ".to_string()
    };

    let mut out = format!(
        "{}{} summaries available. \
         Call get_project_context with the ids you want to load:\n\n",
        header,
        entries.len()
    );

    for (id, titles, tags) in &entries {
        out.push_str(&format!("- id: `{}`\n", id));
        if !titles.is_empty() {
            out.push_str(&format!("  insights: {}\n", titles.join(" | ")));
        }
        if !tags.is_empty() {
            out.push_str(&format!("  tags: {}\n", tags.join(", ")));
        }
    }

    out
}

/// Lightly parse a `.summary.md` to extract insight titles and tags for the index.
fn parse_insight_metadata(content: &str) -> (Vec<String>, Vec<String>) {
    let mut titles: Vec<String> = Vec::new();
    let mut all_tags: Vec<String> = Vec::new();

    for line in content.lines() {
        let t = line.trim();
        // "- **type** Title — summary"
        if let Some(rest) = t.strip_prefix("- **") {
            if let Some(end_bold) = rest.find("**") {
                let after_type = rest[end_bold + 2..].trim();
                let title = after_type
                    .split(" — ")
                    .next()
                    .unwrap_or(after_type)
                    .trim_end_matches('.')
                    .trim();
                if !title.is_empty() {
                    titles.push(title.to_string());
                }
            }
        }
        // "  - tags: tag1, tag2"
        if let Some(tag_str) = t.strip_prefix("- tags: ") {
            for tag in tag_str.split(',') {
                let tag = tag.trim().to_string();
                if !tag.is_empty() && !all_tags.contains(&tag) {
                    all_tags.push(tag);
                }
            }
        }
    }

    (titles, all_tags)
}

fn collect_all_summaries(dir: &Path) -> String {
    if !dir.exists() {
        return format!(
            "No context stored for this project. \
             Call fetch_project_context first.\n(looked in {})",
            dir.display()
        );
    }

    let mut summaries: Vec<String> = Vec::new();
    for entry in WalkDir::new(dir).follow_links(false).sort_by_file_name() {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if e.file_name().to_str().unwrap_or("").ends_with(".summary.md") {
            if let Ok(content) = std::fs::read_to_string(e.path()) {
                summaries.push(content.trim().to_string());
            }
        }
    }

    if summaries.is_empty() {
        return format!(
            "No summaries found. Call fetch_project_context first.\n(looked in {})",
            dir.display()
        );
    }

    summaries.join("\n\n---\n\n")
}

fn collect_summaries_by_ids(proj_dir: &Path, ids: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        // Normalise path separators so ids from the index work on all platforms
        let relative = PathBuf::from(id.replace('\\', "/"));
        let path = proj_dir.join(&relative);
        match std::fs::read_to_string(&path) {
            Ok(content) => out.push(content.trim().to_string()),
            Err(_) => out.push(format!("(summary not found: {})", id)),
        }
    }
    if out.is_empty() {
        "No summaries found for the given ids.".to_string()
    } else {
        out.join("\n\n---\n\n")
    }
}

fn search_summaries(root: &Path, query: &str) -> String {
    if query.trim().is_empty() {
        return "Please provide a non-empty search query.".to_string();
    }
    let q = query.to_lowercase();
    let mut hits: Vec<String> = Vec::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if !e.file_name().to_str().unwrap_or("").ends_with(".summary.md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let matching: Vec<&str> = content
            .lines()
            .filter(|l| l.to_lowercase().contains(&q))
            .collect();
        if !matching.is_empty() {
            hits.push(format!(
                "**{}**\n{}",
                e.path().display(),
                matching
                    .iter()
                    .map(|l| format!("  {}", l.trim()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }

    if hits.is_empty() {
        format!("No matches found for '{query}'.")
    } else {
        format!(
            "Found {} match(es) for '{query}':\n\n{}",
            hits.len(),
            hits.join("\n\n")
        )
    }
}

fn list_projects(data_dir: &Path) -> String {
    let projects_root = data_dir.join("projects");
    if !projects_root.exists() {
        return "No projects found. Call fetch_project_context to initialize.".to_string();
    }
    let Ok(entries) = std::fs::read_dir(&projects_root) else {
        return "Could not read projects directory.".to_string();
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    let mut lines: Vec<String> = Vec::new();
    for entry in entries {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let proj_dir = entry.path();
        let (root_path, created_at) =
            if let Ok(s) = std::fs::read_to_string(proj_dir.join("project.json")) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    (
                        v["root_path"].as_str().unwrap_or("unknown").to_string(),
                        v["created_at"].as_str().unwrap_or("").to_string(),
                    )
                } else {
                    ("unknown".to_string(), String::new())
                }
            } else {
                ("unknown".to_string(), String::new())
            };

        let summary_count = WalkDir::new(&proj_dir)
            .follow_links(false)
            .into_iter()
            .flatten()
            .filter(|e| {
                e.file_type().is_file()
                    && e.file_name().to_str().unwrap_or("").ends_with(".summary.md")
            })
            .count();

        lines.push(format!(
            "- **{}**  \n  path: {}  \n  summaries: {}  \n  initialized: {}",
            entry.file_name().to_string_lossy(),
            root_path,
            summary_count,
            created_at,
        ));
    }

    if lines.is_empty() {
        "No projects found.".to_string()
    } else {
        format!("## Projects ({})\n\n{}", lines.len(), lines.join("\n\n"))
    }
}

pub async fn run_server(data_dir: Option<PathBuf>) -> Result<()> {
    let data_dir = data_dir.unwrap_or_else(default_out_dir);
    let server = ContextPoolServer::new(data_dir);
    let transport = rmcp::transport::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
