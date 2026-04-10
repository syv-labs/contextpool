//! Command implementations for `cxp auth`, `cxp push`, `cxp pull`, and `cxp team`.

use anyhow::{Context, Result};
use std::{fs, path::Path};
use walkdir::WalkDir;

use crate::{
    cli::{AuthArgs, PullArgs, PushArgs, TeamArgs, TeamAction},
    cloud::{self, PushInsight, PushRequest},
    paths::default_out_dir,
    project::project_id_from_path,
    redact::redact_secrets,
};

// ── cxp auth ────────────────────────────────────────────────────────────────

pub async fn cmd_auth(args: AuthArgs) -> Result<()> {
    if args.logout {
        cloud::delete_team_api_key()?;
        println!("Logged out. Team API key removed from keychain.");
        return Ok(());
    }

    if args.status || args.api_key.is_none() {
        return show_auth_status().await;
    }

    let key = args.api_key.unwrap();
    let key = key.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("API key cannot be empty.");
    }

    // Verify the key works BEFORE saving — don't depend on keychain round-trip
    match cloud::get_team_info_with_key(&key).await {
        Ok(info) => {
            // Key is valid — now save it
            cloud::save_team_api_key(&key)?;
            println!("Authenticated as team \"{}\" ({} plan).", info.name, info.plan);
            println!(
                "  {} insight(s), {} project(s), {} member(s).",
                info.usage.insights, info.usage.projects, info.usage.members
            );
            println!("\nTeam key saved.");
        }
        Err(e) => {
            anyhow::bail!("Authentication failed: {e}\nThe key was NOT saved.");
        }
    }

    Ok(())
}

async fn show_auth_status() -> Result<()> {
    match cloud::load_team_api_key() {
        None => {
            println!("Not authenticated. Run `cxp auth <team-key>` to connect to a team.");
        }
        Some(key) => {
            let masked = if key.len() > 12 {
                format!("{}...{}", &key[..8], &key[key.len() - 4..])
            } else {
                "****".to_string()
            };

            match cloud::get_team_info().await {
                Ok(info) => {
                    println!("Team:     {}", info.name);
                    println!("Plan:     {}", info.plan);
                    println!(
                        "Usage:    {} insight(s) / {} limit",
                        info.usage.insights, info.usage.limits.insights
                    );
                    println!(
                        "          {} project(s) / {} limit",
                        info.usage.projects, info.usage.limits.projects
                    );
                    println!(
                        "          {} member(s) / {} limit",
                        info.usage.members, info.usage.limits.members
                    );
                    println!("Key:      {}", masked);
                }
                Err(e) => {
                    println!("Key:      {} (stored but could not verify: {e})", masked);
                }
            }
        }
    }
    Ok(())
}

// ── cxp push ────────────────────────────────────────────────────────────────

pub async fn cmd_push(args: PushArgs) -> Result<()> {
    let _ = cloud::load_team_api_key().ok_or_else(|| {
        anyhow::anyhow!("Not authenticated. Run `cxp auth <team-key>` first.")
    })?;

    let project_dirs = if args.all {
        discover_all_project_dirs()?
    } else {
        let cwd = std::env::current_dir().context("Could not determine current directory")?;
        let project_id = project_id_from_path(&cwd);
        let base = default_out_dir();
        let proj_dir = base.join("projects").join(&project_id);

        // Also check local ContextPool directory
        let local_dir = cwd.join("ContextPool").join("projects").join(&project_id);

        let mut dirs = Vec::new();
        if proj_dir.exists() {
            dirs.push((project_id.clone(), proj_dir));
        }
        if local_dir.exists() {
            dirs.push((project_id, local_dir));
        }
        if dirs.is_empty() {
            anyhow::bail!(
                "No ContextPool data found for this project.\n\
                 Run `cxp init claude-code` or `cxp init cursor` first."
            );
        }
        dirs
    };

    let contributor = git_user_email();

    let mut total_inserted = 0u64;
    let mut total_skipped = 0u64;

    for (project_id, proj_dir) in &project_dirs {
        let insights = collect_insights_from_dir(proj_dir);
        if insights.is_empty() {
            continue;
        }

        let display_name = project_id
            .rsplit('-')
            .next()
            .unwrap_or(project_id)
            .to_string();

        if args.dry_run {
            println!("\n[dry-run] Project: {} ({} insight(s))", project_id, insights.len());
            for (i, ins) in insights.iter().enumerate().take(10) {
                println!("  {}: [{}] {} — {}", i + 1, ins.r#type, ins.title, ins.summary);
            }
            if insights.len() > 10 {
                println!("  ...and {} more", insights.len() - 10);
            }
            continue;
        }

        let req = PushRequest {
            project_id: project_id.clone(),
            project_display_name: Some(display_name),
            source_ide: None,
            contributor: contributor.clone(),
            insights,
        };

        match cloud::push_insights(&req).await {
            Ok(resp) => {
                total_inserted += resp.inserted;
                total_skipped += resp.skipped;
                println!(
                    "  {} — pushed {} new, {} already synced",
                    project_id, resp.inserted, resp.skipped
                );
            }
            Err(e) => {
                eprintln!("  {} — push failed: {e}", project_id);
            }
        }
    }

    if args.dry_run {
        println!("\nDry run complete. No data was sent.");
    } else {
        println!(
            "\nPushed {} new insight(s) ({} already synced).",
            total_inserted, total_skipped
        );
    }

    Ok(())
}

// ── cxp pull ────────────────────────────────────────────────────────────────

pub async fn cmd_pull(args: PullArgs) -> Result<()> {
    let _ = cloud::load_team_api_key().ok_or_else(|| {
        anyhow::anyhow!("Not authenticated. Run `cxp auth <team-key>` first.")
    })?;

    if args.all {
        let projects = cloud::list_team_projects().await?;
        if projects.projects.is_empty() {
            println!("No projects in the team cloud yet. Push some first with `cxp push`.");
            return Ok(());
        }
        for proj in &projects.projects {
            pull_project(&proj.project_id, &proj.display_name).await?;
        }
    } else {
        let cwd = std::env::current_dir().context("Could not determine current directory")?;
        let project_id = project_id_from_path(&cwd);
        let display_name = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&project_id)
            .to_string();
        pull_project(&project_id, &display_name).await?;
    }

    Ok(())
}

async fn pull_project(project_id: &str, display_name: &str) -> Result<()> {
    let resp = cloud::pull_insights(project_id).await?;

    if resp.insights.is_empty() {
        println!("  {} — no team insights found", display_name);
        return Ok(());
    }

    // Write team insights to a local cache directory
    let cache_dir = cache_dir_for_project(project_id);
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Creating cache dir {}", cache_dir.display()))?;

    // Write as a single team-insights.md file
    let mut md = String::from("# Team Insights\n\n");
    for ins in &resp.insights {
        let ty = if ins.r#type.is_empty() { "insight" } else { &ins.r#type };
        md.push_str(&format!(
            "- **{}** {} — {}\n  - contributor: {}\n  - tags: {}\n",
            ty,
            ins.title,
            ins.summary,
            ins.contributor,
            ins.tags.join(", ")
        ));
        if let Some(f) = &ins.file {
            if !f.is_empty() {
                md.push_str(&format!("  - file: `{}`\n", f));
            }
        }
    }

    let out_path = cache_dir.join("team-insights.md");
    fs::write(&out_path, &md)?;

    println!(
        "  {} — pulled {} insight(s) from team",
        display_name,
        resp.insights.len()
    );

    Ok(())
}

pub(crate) fn cache_dir_for_project(project_id: &str) -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
        .join("contextpool")
        .join("team-cache")
        .join(project_id)
}

// ── cxp team ────────────────────────────────────────────────────────────────

pub async fn cmd_team(args: TeamArgs) -> Result<()> {
    match args.action {
        Some(TeamAction::Projects) => {
            let resp = cloud::list_team_projects().await?;
            if resp.projects.is_empty() {
                println!("No projects in the team cloud yet.");
                return Ok(());
            }
            println!("{:<40} {:>8}  {}", "PROJECT", "INSIGHTS", "CREATED");
            println!("{}", "-".repeat(70));
            for p in &resp.projects {
                println!(
                    "{:<40} {:>8}  {}",
                    p.display_name, p.insight_count, p.created_at
                );
            }
        }
        None => {
            // Default: show team info
            let info = cloud::get_team_info().await?;
            println!("Team:      {}", info.name);
            println!("Plan:      {}", info.plan);
            println!(
                "Insights:  {} / {}",
                info.usage.insights, info.usage.limits.insights
            );
            println!(
                "Projects:  {} / {}",
                info.usage.projects, info.usage.limits.projects
            );
            println!(
                "Members:   {} / {}",
                info.usage.members, info.usage.limits.members
            );
        }
    }
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Parse insight lines from `.summary.md` files into structured PushInsight records.
pub(crate) fn collect_insights_from_dir(dir: &Path) -> Vec<PushInsight> {
    let mut insights = Vec::new();

    for entry in WalkDir::new(dir).follow_links(false).sort_by_file_name() {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        if !e.file_name().to_str().unwrap_or("").ends_with(".summary.md") {
            continue;
        }
        let Ok(content) = fs::read_to_string(e.path()) else {
            continue;
        };

        for line in content.lines() {
            let t = line.trim();
            // Parse "- **type** Title — summary" format
            let Some(rest) = t.strip_prefix("- **") else {
                continue;
            };
            let Some(end_bold) = rest.find("**") else {
                continue;
            };
            let ty = rest[..end_bold].trim().to_string();
            let after = rest[end_bold + 2..].trim().to_string();
            if after.is_empty() {
                continue;
            }

            let (title, summary) = if let Some(dash_pos) = after.find(" — ") {
                (
                    after[..dash_pos].trim().to_string(),
                    after[dash_pos + " — ".len()..].trim().to_string(),
                )
            } else {
                (after, String::new())
            };

            // Double-redact before pushing — defense in depth
            let title = redact_secrets(&title);
            let summary = redact_secrets(&summary);

            // Parse tags from the next line if present (handled inline for simplicity)
            insights.push(PushInsight {
                r#type: ty,
                title,
                summary,
                tags: Vec::new(), // Tags will be parsed from the "tags:" line below
                file: None,
            });
        }

        // Second pass: attach tags and file to the insight above them
        let mut idx = 0usize;
        let insight_count = insights.len();
        let start_idx = insight_count.saturating_sub(
            content
                .lines()
                .filter(|l| l.trim().starts_with("- **"))
                .count(),
        );
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with("- **") {
                idx = start_idx + insights[start_idx..].iter().position(|_| true).unwrap_or(0);
                // This is handled above, just track position
                idx += 1;
                continue;
            }
            if idx == 0 || idx - 1 >= insights.len() {
                continue;
            }
            let target = idx - 1;
            if let Some(tags_str) = t.strip_prefix("- tags: ") {
                insights[target].tags = tags_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(file_str) = t.strip_prefix("- file: `") {
                if let Some(file_str) = file_str.strip_suffix('`') {
                    insights[target].file = Some(file_str.to_string());
                }
            }
        }
    }

    insights
}

/// Discover all local project directories with summaries.
fn discover_all_project_dirs() -> Result<Vec<(String, std::path::PathBuf)>> {
    let base = default_out_dir();
    let projects_dir = base.join("projects");
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut dirs = Vec::new();
    for entry in fs::read_dir(&projects_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let project_id = entry.file_name().to_string_lossy().to_string();
            dirs.push((project_id, entry.path()));
        }
    }

    Ok(dirs)
}

/// Get the current git user.email for contributor attribution.
pub(crate) fn git_user_email() -> Option<String> {
    std::process::Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let email = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if email.is_empty() {
                    None
                } else {
                    Some(email)
                }
            } else {
                None
            }
        })
}

// ── Reusable sync helpers (used by MCP auto-sync) ──────────────────────────

/// Pull team insights and write them as a `.summary.md` file into `dest_dir`.
/// Returns the number of insights pulled, or an error.
/// Suitable for use from the MCP server (no stdout output).
pub(crate) async fn pull_insights_to_dir(
    project_id: &str,
    dest_dir: &Path,
) -> Result<usize> {
    let resp = cloud::pull_insights(project_id).await?;
    if resp.insights.is_empty() {
        return Ok(0);
    }

    fs::create_dir_all(dest_dir)?;

    // Format as .summary.md so search_summaries picks it up automatically
    let mut md = String::from(
        "# Summary\n\n\
         ## Metadata\n\
         - **Source:** Team cloud\n\
         - **Session:** team-insights\n\n\
         ## Extracted insights\n\n",
    );

    for ins in &resp.insights {
        let ty = if ins.r#type.is_empty() { "insight" } else { &ins.r#type };
        md.push_str(&format!(
            "- **{}** {} — {}\n",
            ty, ins.title, ins.summary,
        ));
        if let Some(f) = &ins.file {
            if !f.is_empty() {
                md.push_str(&format!("  - file: `{}`\n", f));
            }
        }
        if !ins.tags.is_empty() {
            md.push_str(&format!("  - tags: {}\n", ins.tags.join(", ")));
        }
    }

    let out_path = dest_dir.join("team-insights.summary.md");
    fs::write(&out_path, &md)?;

    // Also write to the legacy cache dir for CLI compatibility
    let cache_dir = cache_dir_for_project(project_id);
    let _ = fs::create_dir_all(&cache_dir);
    let mut legacy_md = String::from("# Team Insights\n\n");
    for ins in &resp.insights {
        let ty = if ins.r#type.is_empty() { "insight" } else { &ins.r#type };
        legacy_md.push_str(&format!(
            "- **{}** {} — {}\n  - contributor: {}\n  - tags: {}\n",
            ty,
            ins.title,
            ins.summary,
            ins.contributor,
            ins.tags.join(", ")
        ));
        if let Some(f) = &ins.file {
            if !f.is_empty() {
                legacy_md.push_str(&format!("  - file: `{}`\n", f));
            }
        }
    }
    let _ = fs::write(cache_dir.join("team-insights.md"), &legacy_md);

    Ok(resp.insights.len())
}

/// Push local insights from `proj_dir` to the cloud. Returns (inserted, skipped).
/// Suitable for background use from MCP server (no stdout output).
pub(crate) async fn push_insights_from_dir(
    project_id: &str,
    proj_dir: &Path,
) -> Result<(u64, u64)> {
    let insights = collect_insights_from_dir(proj_dir);
    if insights.is_empty() {
        return Ok((0, 0));
    }

    let display_name = project_id
        .rsplit('-')
        .next()
        .unwrap_or(project_id)
        .to_string();

    let req = PushRequest {
        project_id: project_id.to_string(),
        project_display_name: Some(display_name),
        source_ide: None,
        contributor: git_user_email(),
        insights,
    };

    let resp = cloud::push_insights(&req).await?;
    Ok((resp.inserted, resp.skipped))
}
