use crate::{
    cli::{InitClaudeCodeArgs, InitCursorArgs},
    export::{
        claude_code::{claude_code_project_dir_name, export_claude_code_project_sessions},
        cursor::export_cursor_project_chats,
    },
    paths::{default_claude_code_dir, default_cursor_dir, default_out_dir},
    project::{project_dir, project_id_from_path},
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::{fs, path::Path};
use walkdir::WalkDir;

pub async fn init_claude_code(args: InitClaudeCodeArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("Could not determine current directory")?;
    let project_id = project_id_from_path(&cwd);

    let base = if args.local {
        cwd.join("ContextPool")
    } else {
        args.out.unwrap_or_else(|| default_out_dir())
    };
    fs::create_dir_all(&base).with_context(|| format!("Creating {}", base.display()))?;

    let proj_dir = project_dir(&base, &project_id);
    fs::create_dir_all(&proj_dir).with_context(|| format!("Creating {}", proj_dir.display()))?;

    let meta_path = proj_dir.join("project.json");
    if !meta_path.exists() {
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let meta = serde_json::json!({
            "project_id": project_id,
            "root_path": cwd.to_string_lossy(),
            "created_at": created_at,
        });
        fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    let claude_dir = args
        .claude_dir
        .or_else(|| default_claude_code_dir())
        .context("Could not determine Claude Code directory (try --claude-dir)")?;

    let cc_project_dir_name = claude_code_project_dir_name(&cwd);

    let run_dir = proj_dir.join("imports").join("claude-code");
    fs::create_dir_all(&run_dir)?;

    let imported = export_claude_code_project_sessions(
        &claude_dir,
        &cc_project_dir_name,
        &args.session_ids,
        &run_dir,
    )
    .await?;

    print_aha_preview(&proj_dir, "Claude Code", imported);
    Ok(())
}

pub async fn init_cursor(args: InitCursorArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("Could not determine current directory")?;
    let project_id = project_id_from_path(&cwd);

    let base = if args.local {
        cwd.join("ContextPool")
    } else {
        args.out.unwrap_or_else(|| default_out_dir())
    };
    fs::create_dir_all(&base).with_context(|| format!("Creating {}", base.display()))?;

    let proj_dir = project_dir(&base, &project_id);
    fs::create_dir_all(&proj_dir).with_context(|| format!("Creating {}", proj_dir.display()))?;

    let meta_path = proj_dir.join("project.json");
    if !meta_path.exists() {
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let meta = serde_json::json!({
            "project_id": project_id,
            "root_path": cwd.to_string_lossy(),
            "created_at": created_at,
        });
        fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    let cursor_dir = args
        .cursor_dir
        .or_else(|| default_cursor_dir())
        .context("Could not determine Cursor directory (try --cursor-dir)")?;

    let run_dir = proj_dir.join("imports").join("cursor");
    fs::create_dir_all(&run_dir)?;

    let imported = export_cursor_project_chats(
        &cursor_dir,
        &project_id,
        &args.chat_ids,
        &run_dir,
    )
    .await?;

    print_aha_preview(&proj_dir, "Cursor", imported);
    Ok(())
}

/// Print a rich preview of extracted insights after init.
fn print_aha_preview(proj_dir: &Path, source: &str, sessions_imported: usize) {
    // Collect all insights from summary files
    let mut insights: Vec<(String, String)> = Vec::new(); // (type, title_or_summary)

    for entry in WalkDir::new(proj_dir).follow_links(false).sort_by_file_name() {
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
            // Parse "- **type** Title — summary" lines
            if let Some(rest) = t.strip_prefix("- **") {
                if let Some(end_bold) = rest.find("**") {
                    let ty = rest[..end_bold].to_string();
                    let after = rest[end_bold + 2..].trim().to_string();
                    if !after.is_empty() {
                        insights.push((ty, after));
                    }
                }
            }
        }
    }

    let summary_count = WalkDir::new(proj_dir)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name().to_str().unwrap_or("").ends_with(".summary.md")
        })
        .count();

    println!();
    println!("  Found {} {} session(s) for this project.", sessions_imported, source);
    println!(
        "  Summarized {} session(s) -> {} insight(s) extracted.",
        summary_count,
        insights.len()
    );

    if !insights.is_empty() {
        println!();
        println!("  Top insights:");
        for (ty, text) in insights.iter().take(8) {
            // Truncate long lines for terminal display
            let display = if text.len() > 100 {
                format!("{}...", &text[..97])
            } else {
                text.clone()
            };
            println!("    {}: {}", ty, display);
        }
        if insights.len() > 8 {
            println!("    ...and {} more", insights.len() - 8);
        }
    }

    println!();
    println!("  Your agent will now recall these automatically via MCP.");
    println!(
        "  Stored at: {}",
        proj_dir.display()
    );
    println!();
}
