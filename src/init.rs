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
use std::fs;

pub async fn init_claude_code(args: InitClaudeCodeArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("Could not determine current directory")?;
    let project_id = project_id_from_path(&cwd);

    let base = args.out.unwrap_or_else(|| default_out_dir());
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

    // Claude Code encodes the project path as the directory name under ~/.claude/projects/
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

    println!(
        "Initialized project {}\nStored at {}\nImported {} Claude Code session(s).",
        project_id,
        proj_dir.display(),
        imported
    );
    Ok(())
}

pub async fn init_cursor(args: InitCursorArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("Could not determine current directory")?;
    let project_id = project_id_from_path(&cwd);

    let base = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&base).with_context(|| format!("Creating {}", base.display()))?;

    let proj_dir = project_dir(&base, &project_id);
    fs::create_dir_all(&proj_dir).with_context(|| format!("Creating {}", proj_dir.display()))?;

    // Minimal metadata so we can map id -> original path later.
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

    println!(
        "Initialized project {}\nStored at {}\nImported {} Cursor transcript(s).",
        project_id,
        proj_dir.display(),
        imported
    );
    Ok(())
}
