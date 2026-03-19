use crate::{
    cli::ExportVscdbArgs,
    paths::{default_out_dir, default_workspace_storage_dir},
    summarize::{fallback_summary, summarize_via_api},
    transcript::extract_text_from_json,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub async fn export_vscdb(args: ExportVscdbArgs) -> Result<()> {
    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir
        .join("exports")
        .join("vscdb")
        .join(args.product.replace(['\\', '/'], "_"))
        .join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let workspace_storage = args
        .workspace_storage
        .or_else(|| default_workspace_storage_dir(&args.product))
        .context("Could not determine workspaceStorage directory (try --workspace-storage)")?;

    let api_base = args
        .api_base
        .or_else(|| std::env::var("CONTEXT_POOL_API_BASE").ok());
    let api_key: Option<String> = None;

    let db_paths = discover_state_vscdbs(&workspace_storage)?;
    let mut index: Vec<ExportedItem> = Vec::new();

    for db_path in db_paths {
        let extracted = extract_chat_text_from_state_vscdb(&db_path).unwrap_or_default();
        if extracted.trim().is_empty() {
            continue;
        }

        let summary = if args.offline {
            fallback_summary(&extracted)
        } else {
            summarize_via_api(&extracted, api_base.as_deref(), api_key.as_deref())
                .await
                .unwrap_or_else(|_| fallback_summary(&extracted))
        };

        let safe_name = safe_rel_name(&workspace_storage, &db_path);
        let out_file = run_dir.join(format!("{safe_name}.summary.md"));
        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(
            &out_file,
            format!(
                "# Summary\n\n{}\n\n## Source\n- `{}`\n",
                summary.trim(),
                db_path.display()
            ),
        )?;

        index.push(ExportedItem {
            source_path: db_path.to_string_lossy().to_string(),
            output_path: out_file.to_string_lossy().to_string(),
            chars_in: extracted.len(),
        });
    }

    let index_path = run_dir.join("index.json");
    fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;

    println!(
        "Exported {} workspace DB(s) to {}",
        index.len(),
        run_dir.display()
    );

    Ok(())
}

fn discover_state_vscdbs(workspace_storage: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !workspace_storage.exists() {
        return Ok(found);
    }
    for entry in WalkDir::new(workspace_storage).follow_links(false).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() == "state.vscdb" {
            found.push(entry.into_path());
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

fn extract_chat_text_from_state_vscdb(db_path: &Path) -> Result<String> {
    // Open read-only so we never mutate editor state.
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("Opening sqlite db {}", db_path.display()))?;

    // Known keys used by Cursor / VS Code forks for AI chat data.
    // Community reports mention these keys frequently.
    let keys = [
        "workbench.panel.aichat.view.aichat.chatdata",
        "aiService.prompts",
        "composerData",
    ];

    let mut out = String::new();

    for key in keys {
        let mut stmt = conn.prepare("SELECT value FROM ItemTable WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        while let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            let extracted = extract_text_from_json(&val);
            if extracted.trim().is_empty() {
                continue;
            }
            out.push_str(&format!("== {key} ==\n"));
            out.push_str(&extracted);
            out.push('\n');
        }
    }

    Ok(out)
}

fn safe_rel_name(root: &Path, full: &Path) -> String {
    let rel = full.strip_prefix(root).unwrap_or(full);
    rel.to_string_lossy()
        .replace(['\\', '/'], "__")
        .trim_matches('_')
        .to_string()
}

#[derive(serde::Serialize)]
struct ExportedItem {
    source_path: String,
    output_path: String,
    chars_in: usize,
}

