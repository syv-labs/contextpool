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
        Annotated, CallToolRequestParam, CallToolResult, Content, Implementation,
        ListResourcesResult, ListToolsResult, PaginatedRequestParamInner, ProtocolVersion,
        RawResource, ReadResourceRequestParam, ReadResourceResult, ResourceContents,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    Error as McpError, RoleServer, ServerHandler, ServiceExt,
};
use walkdir::WalkDir;

use tokio::time::{timeout, Duration};

use crate::{
    embedded_agent::{ContextItem, EmbeddedAgentOptions},
    paths::default_out_dir,
    project::{project_dir, project_id_from_path},
    team,
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
        let path = match resolve_project_path(project_path) {
            Ok(p) => p,
            Err(e) => return e,
        };

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

        // ── Cloud sync: pull team insights ──
        let mut sync_status = String::new();
        let has_api_key = crate::cloud::load_team_api_key().is_some();

        if has_api_key {
            let team_dir = proj_dir.join("team");
            match timeout(
                Duration::from_secs(10),
                team::pull_insights_to_dir(&project_id, &team_dir),
            )
            .await
            {
                Ok(Ok(count)) if count > 0 => {
                    sync_status.push_str(&format!("Pulled {} team insight(s). ", count));
                }
                Ok(Ok(_)) => {
                    sync_status.push_str("Team cloud: up to date. ");
                }
                Ok(Err(e)) => {
                    eprintln!("cloud pull: {e}");
                }
                Err(_) => {
                    eprintln!("cloud pull: timed out");
                }
            }
        }

        // Require API key to be pre-configured (keychain / env) — no interactive prompt in MCP.
        let backend = match crate::credentials::load_api_backend() {
            Some(b) => b,
            None => {
                return "No summarization backend available.\n\n\
                    Inside Claude Code: this should work automatically (uses `claude` CLI).\n\
                    If it doesn't, ensure the `claude` binary is in your PATH.\n\n\
                    Inside Cursor: set one of these env vars in your MCP config:\n\
                    ANTHROPIC_API_KEY, OPENAI_API_KEY, or NVIDIA_API_KEY"
                    .to_string()
            }
        };

        // Tier 8: Clean stale index entries where summary files were deleted
        clean_stale_index_entries(&proj_dir);

        // Collect transcript files not yet indexed
        let already_indexed = collect_indexed_source_paths(&proj_dir);
        let new_files = discover_new_transcripts(&path, &project_id, &already_indexed);

        if new_files.is_empty() {
            let mut result = format_context_index(&proj_dir, 0, 0, 0);
            if has_api_key {
                let push_pid = project_id.clone();
                let push_dir = proj_dir.clone();
                tokio::spawn(async move {
                    match team::push_insights_from_dir(&push_pid, &push_dir).await {
                        Ok((ins, _)) if ins > 0 => eprintln!("cloud push: {} new insight(s) synced", ins),
                        Ok(_) => {}
                        Err(e) => eprintln!("cloud push: {e}"),
                    }
                });
                sync_status.push_str("Synced with team cloud. ");
            }
            if !sync_status.is_empty() {
                result.push_str(&format!("\n\n{}", sync_status.trim()));
            }
            return result;
        }

        // Summarize new transcripts in parallel.
        // opts/run_dir/now are wrapped in Arc so each task can hold a cheap clone.
        let opts = Arc::new(EmbeddedAgentOptions::from_env(backend));
        let run_id = Utc::now()
            .to_rfc3339_opts(SecondsFormat::Secs, true)
            .replace(':', "-");
        let run_dir = proj_dir.join("fetched").join(&run_id);
        if let Err(e) = std::fs::create_dir_all(&run_dir) {
            return format!("Failed to create run dir: {e}");
        }
        let run_dir = Arc::new(run_dir);

        let mut index_entries: Vec<serde_json::Value> = Vec::new();
        let mut imported = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut errors: Vec<String> = Vec::new();
        let now = Arc::new(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));

        // Limit concurrent LLM calls to avoid rate-limit errors.
        // Override with CXP_CONCURRENCY env var (default 8).
        let concurrency = std::env::var("CXP_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(8)
            .max(1);
        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));

        // Return type per task:
        //   None                            → I/O error, skip (retry next fetch)
        //   Some((entry, true,  false, None))  → imported successfully
        //   Some((entry, false, true,  None))  → skipped (too short)
        //   Some((_,     false, false, None))  → LLM ran, no insights (mark indexed)
        //   Some((_,     false, false, Some(e))) → LLM failed (retry next fetch)
        type TaskOut = Option<(serde_json::Value, bool, bool, Option<String>)>;
        let mut set: tokio::task::JoinSet<TaskOut> = tokio::task::JoinSet::new();

        for transcript_path in new_files {
            let opts = opts.clone();
            let run_dir = run_dir.clone();
            let now = now.clone();
            let sem = sem.clone();

            set.spawn(async move {
                // Acquire a slot — released automatically when _permit is dropped.
                let _permit = sem.acquire_owned().await;

                let raw = match std::fs::read_to_string(&transcript_path) {
                    Ok(r) => r,
                    Err(_) => return None,
                };
                let extracted = crate::transcript::extract_text_from_jsonl(&raw);
                let src = transcript_path.to_string_lossy().to_string();

                // Tier 5: Skip tiny transcripts
                if extracted.len() < 100 {
                    let entry = serde_json::json!({
                        "source_path": src,
                        "output_path": null,
                        "chars_in": extracted.len(),
                        "skipped": "too short",
                    });
                    return Some((entry, false, true, None));
                }

                let (items, _) = match crate::embedded_agent::generate_context_items(
                    &extracted, &[], "", &opts,
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        // Tier 1: Do NOT mark as indexed — will retry on next fetch.
                        return Some((serde_json::Value::Null, false, false, Some(e.to_string())));
                    }
                };

                // No insights — mark indexed (LLM ran fine, nothing worth storing)
                if items.is_empty() {
                    let entry = serde_json::json!({
                        "source_path": src,
                        "output_path": null,
                        "chars_in": extracted.len(),
                    });
                    return Some((entry, false, false, None));
                }

                let safe_name = transcript_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("session")
                    .to_string();
                let out_file = run_dir.join(format!("{safe_name}.summary.md"));

                // Tier 4: Add metadata & attribution
                let source_ide = if src.contains(".claude/projects") {
                    "Claude Code"
                } else if src.contains(".cursor/") {
                    "Cursor"
                } else {
                    "Unknown"
                };

                let summary_md = items_to_markdown(&items);
                let _ = std::fs::write(
                    &out_file,
                    format!(
                        "# Summary\n\n\
                         ## Metadata\n\
                         - **Source:** {source_ide} session\n\
                         - **Session:** {safe_name}\n\
                         - **Indexed:** {now}\n\n\
                         {summary_md}\n\n\
                         ## Source\n- `{src}`\n"
                    ),
                );

                let entry = serde_json::json!({
                    "source_path": src,
                    "output_path": out_file.to_string_lossy(),
                    "chars_in": extracted.len(),
                });
                Some((entry, true, false, None))
            });
        }

        // Collect results as tasks complete.
        while let Some(join_result) = set.join_next().await {
            match join_result.unwrap_or(None) {
                None => {} // I/O error — skip, retry on next fetch
                Some((_, false, false, Some(e))) => {
                    failed += 1;
                    errors.push(e);
                }
                Some((entry, false, true, None)) => {
                    skipped += 1;
                    index_entries.push(entry);
                }
                Some((entry, true, false, None)) => {
                    imported += 1;
                    index_entries.push(entry);
                }
                Some((entry, false, false, None)) => {
                    index_entries.push(entry); // LLM ran, no insights
                }
                Some(_) => {} // unreachable
            }
        }

        if !index_entries.is_empty() {
            let index_path = run_dir.join("index.json");
            let _ = atomic_write(
                &index_path,
                &serde_json::to_string_pretty(&index_entries).unwrap_or_default(),
            );
        }

        let mut result = format_context_index(&proj_dir, imported, failed, skipped);

        // Tier 1: Surface errors
        if !errors.is_empty() {
            result.push_str("\n\nErrors:\n");
            for (i, e) in errors.iter().enumerate().take(5) {
                result.push_str(&format!("{}. {}\n", i + 1, e));
            }
            if errors.len() > 5 {
                result.push_str(&format!("  ...and {} more\n", errors.len() - 5));
            }
        }

        // ── Cloud sync: push local insights (background) ──
        if has_api_key {
            let push_pid = project_id.clone();
            let push_dir = proj_dir.clone();
            tokio::spawn(async move {
                match team::push_insights_from_dir(&push_pid, &push_dir).await {
                    Ok((ins, _)) if ins > 0 => eprintln!("cloud push: {} new insight(s) synced", ins),
                    Ok(_) => {}
                    Err(e) => eprintln!("cloud push: {e}"),
                }
            });
            sync_status.push_str("Synced with team cloud. ");
        }

        if !sync_status.is_empty() {
            result.push_str(&format!("\n\n{}", sync_status.trim()));
        }

        result
    }

    fn get_project_context_impl(
        &self,
        project_path: Option<String>,
        ids: Option<Vec<String>>,
    ) -> String {
        let path = match resolve_project_path(project_path) {
            Ok(p) => p,
            Err(e) => return e,
        };
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
            capabilities: ServerCapabilities::builder().enable_tools().enable_resources().build(),
            server_info: Implementation {
                name: "contextpool".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "ContextPool gives you persistent memory across sessions. \
                 Summaries are stored locally from both Claude Code and Cursor sessions. \
                 Follow these rules:\n\n\
                 INDEXING: IMMEDIATELY call fetch_project_context at the very start of every new \
                 conversation, before responding to the user's first message. It returns only a \
                 count and topic list — not the summaries themselves — so it is safe to call \
                 without consuming context.\n\n\
                 FINDING RELEVANT CONTEXT: Use search_context as the primary way to access memory. \
                 Search with specific keywords related to the current task (error messages, \
                 component names, library names, decisions). Only content that matches is returned, \
                 so context window usage is proportional to relevance.\n\n\
                 LOADING PAST SESSIONS: Only call get_project_context when the user explicitly \
                 asks to review past sessions or when search_context returns no results and you \
                 need broader coverage. Loading everything at once is expensive — prefer search.\n\n\
                 WHEN TO SEARCH: Before suggesting a fix for a bug, before making an architectural \
                 decision, when the user says \"we talked about this\" or \"remember when...\", \
                 or when debugging an error you haven't seen before.\n\n\
                 Use list_context_projects when the user works across multiple repositories."
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
                    description: "Discover and summarize new Cursor and Claude Code transcripts \
                         for this project. Stores summaries locally in <project>/ContextPool/. \
                         Returns only a session count and topic list — NOT the summaries themselves. \
                         Call this once per conversation to ensure indexing is current, then use \
                         search_context to retrieve relevant insights."
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
                         Pass ids to load specific summaries. Omit ids to load all (expensive — \
                         prefer search_context for targeted retrieval). Use this only when the \
                         user explicitly asks to review past sessions or search returns no results."
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
                         This is the primary way to access memory — only matching excerpts \
                         are returned, keeping context usage proportional to relevance. \
                         Search before suggesting fixes, making decisions, or when the user \
                         references past work. Covers both Claude Code and Cursor sessions."
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
                let query = match args.get("query").and_then(|v| v.as_str()) {
                    Some(q) if !q.trim().is_empty() => q.to_string(),
                    _ => {
                        return Err(McpError::invalid_params(
                            "search_context requires a non-empty 'query' parameter",
                            None,
                        ));
                    }
                };
                let project_path = args
                    .get("project_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.search_context_impl(&query, project_path)
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

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParamInner>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            next_cursor: None,
            resources: vec![Annotated {
                raw: RawResource {
                    uri: "contextpool://index".into(),
                    name: "ContextPool Session Index".into(),
                    description: Some(
                        "Current project context index. Reading this resource triggers \
                         background indexing of any new coding sessions."
                            .into(),
                    ),
                    mime_type: Some("text/plain".into()),
                    size: None,
                },
                annotations: None,
            }],
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri != "contextpool://index" {
            return Err(McpError::resource_not_found(
                format!("Unknown resource: {}", request.uri),
                None,
            ));
        }

        let path = std::env::current_dir().map_err(|e| {
            McpError::internal_error(format!("Cannot determine cwd: {e}"), None)
        })?;

        let project_id = project_id_from_path(&path);
        let proj_dir = project_dir(&path.join("ContextPool"), &project_id);

        // Return cached index immediately — no summarization on the hot path.
        let cached = format_context_index(&proj_dir, 0, 0, 0);

        // Kick off background indexing of any new sessions.
        let server = self.clone();
        let path_str = path.to_string_lossy().to_string();
        tokio::spawn(async move {
            server.fetch_project_context_impl(Some(path_str)).await;
        });

        let text = format!(
            "{cached}\n\n\
             *(Background indexing of new sessions started — \
             call search_context or fetch_project_context to get fresh results.)*"
        );

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(text, request.uri)],
        })
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Write `content` to `path` atomically via a sibling `.tmp` file + rename.
/// On POSIX, `rename(2)` is atomic: readers never see a partial write.
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn resolve_project_path(project_path: Option<String>) -> Result<PathBuf, String> {
    if let Some(p) = project_path {
        return Ok(PathBuf::from(p));
    }
    std::env::current_dir().map_err(|e| {
        format!(
            "Cannot determine project directory: {e}.\n\
             Pass the 'project_path' parameter explicitly to fix this."
        )
    })
}

/// Tier 8: Remove index entries where the summary file was deleted,
/// so those transcripts get re-processed on the next fetch.
fn clean_stale_index_entries(proj_dir: &Path) {
    for entry in WalkDir::new(proj_dir).follow_links(false) {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() || e.file_name().to_str() != Some("index.json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&content) else {
            continue;
        };
        let original_len = arr.len();
        let cleaned: Vec<&serde_json::Value> = arr
            .iter()
            .filter(|item| {
                // Keep entries with no output_path (no insights / skipped) — they're valid
                match item["output_path"].as_str() {
                    Some(p) => Path::new(p).exists(),
                    None => true,
                }
            })
            .collect();
        if cleaned.len() < original_len {
            let _ = atomic_write(
                e.path(),
                &serde_json::to_string_pretty(&cleaned).unwrap_or_default(),
            );
        }
    }
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

/// Build the lightweight overview that `fetch_project_context` returns.
///
/// Does NOT list individual summaries — that would flood the context window.
/// The agent should use `search_context` to pull only what is relevant,
/// and `get_project_context` only when the user explicitly asks for past sessions.
fn format_context_index(
    proj_dir: &Path,
    newly_imported: usize,
    failed: usize,
    skipped: usize,
) -> String {
    let mut summary_count = 0usize;
    let mut all_tags: Vec<String> = Vec::new();

    for entry in WalkDir::new(proj_dir).follow_links(false) {
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
        summary_count += 1;
        let (_, tags) = parse_insight_metadata(&content);
        for tag in tags {
            if !all_tags.contains(&tag) {
                all_tags.push(tag);
            }
        }
    }

    // Tier 2: Helpful empty-state message
    if summary_count == 0 {
        return "No transcripts indexed yet for this project.\n\n\
                ContextPool indexes sessions from:\n\
                - Claude Code: ~/.claude/projects/<project>/\n\
                - Cursor: ~/.cursor/projects/<project>/agent-transcripts/\n\n\
                Start a coding session and run fetch_project_context again to index it."
            .to_string();
    }

    // Status line
    let mut parts: Vec<String> = Vec::new();
    if newly_imported > 0 {
        parts.push(format!("{} new session(s) indexed", newly_imported));
    }
    if failed > 0 {
        parts.push(format!("{} failed (will retry)", failed));
    }
    if skipped > 0 {
        parts.push(format!("{} skipped (too short)", skipped));
    }
    if parts.is_empty() {
        parts.push("Up to date".to_string());
    }

    all_tags.sort();
    let topics = if all_tags.is_empty() {
        String::new()
    } else {
        format!("\nTopics covered: {}", all_tags.join(", "))
    };

    format!(
        "{status}. {count} session(s) stored locally (Claude Code + Cursor).{topics}\n\
         Use search_context(\"keyword\") to find relevant insights.",
        status = parts.join(". "),
        count = summary_count,
        topics = topics,
    )
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
            Err(_) => out.push(format!(
                "(summary not found: {})\nHint: call fetch_project_context first to see available summary IDs.",
                id
            )),
        }
    }
    if out.is_empty() {
        "No summaries found for the given ids.".to_string()
    } else {
        out.join("\n\n---\n\n")
    }
}

/// Groups lines of a summary file into insight blocks.
/// Each block is the `- **type** title — summary` line plus any immediately
/// following `  - file:` / `  - tags:` child lines.
fn extract_insight_blocks(content: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("- **") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(format!("  {}", t));
        } else if t.starts_with("- tags:") || t.starts_with("- file:") {
            if let Some(ref mut block) = current {
                block.push('\n');
                block.push_str(&format!("    {}", t));
            }
        } else if current.is_some() && !t.is_empty() {
            // Non-child, non-blank line ends the current block
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn search_summaries(root: &Path, query: &str) -> String {
    if query.trim().is_empty() {
        return "Please provide a non-empty search query.".to_string();
    }

    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    if terms.is_empty() {
        return "Please provide a non-empty search query.".to_string();
    }

    // (match_count, full_match, formatted_hit)
    let mut hits: Vec<(usize, bool, String)> = Vec::new();

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

        let content_lower = content.to_lowercase();

        // At least one term must appear; rank by how many terms match
        let matched_terms = terms
            .iter()
            .filter(|t| content_lower.contains(t.as_str()))
            .count();
        if matched_terms == 0 {
            continue;
        }
        let full_match = matched_terms == terms.len();

        // Extract full insight blocks; include a block if any of its lines
        // contain at least one search term
        let all_blocks = extract_insight_blocks(&content);
        let matching_blocks: Vec<&str> = all_blocks
            .iter()
            .filter(|block| {
                let lower = block.to_lowercase();
                terms.iter().any(|t| lower.contains(t.as_str()))
            })
            .map(String::as_str)
            .collect();

        let id = e
            .path()
            .strip_prefix(root)
            .unwrap_or(e.path())
            .to_string_lossy()
            .to_string();

        let block_text = if matching_blocks.is_empty() {
            // Fallback: show first few matching lines
            content
                .lines()
                .filter(|l| {
                    let lower = l.to_lowercase();
                    terms.iter().any(|t| lower.contains(t.as_str()))
                })
                .take(5)
                .map(|l| format!("  {}", l.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            matching_blocks.join("\n")
        };

        let label = if full_match { "" } else { " *(partial)*" };
        hits.push((
            matched_terms,
            full_match,
            format!("**{}**{}\n{}", id, label, block_text),
        ));
    }

    // Full matches first, then by matched_terms descending
    hits.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));

    if hits.is_empty() {
        format!("No matches found for '{query}'.")
    } else {
        format!(
            "Found {} match(es) for '{query}':\n\n{}",
            hits.len(),
            hits.iter()
                .map(|(_, _, text)| text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
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
