use crate::{
    cli::ExportClaudeCodeArgs,
    paths::{default_claude_code_dir, default_out_dir},
    summarize::{fallback_summary, summarize_embedded},
    transcript::extract_text_from_jsonl,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::{
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub async fn export_claude_code(args: ExportClaudeCodeArgs) -> Result<()> {
    let claude_dir = args
        .claude_dir
        .or_else(|| default_claude_code_dir())
        .context("Could not determine Claude Code directory (try --claude-dir)")?;

    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join("exports").join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let session_paths = if let Some(single) = args.session.clone() {
        vec![single]
    } else {
        discover_claude_code_sessions(&claude_dir)?
    };

    let mut index: Vec<ExportedItem> = Vec::new();
    let total = session_paths.len();

    for (i, path) in session_paths.iter().enumerate() {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("session");
        eprint!("  [{}/{}] {}... ", i + 1, total, name);
        let _ = io::stderr().flush();

        let raw = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
        let extracted = extract_text_from_jsonl(&raw);

        let summary = if args.offline {
            eprintln!("offline");
            fallback_summary(&extracted)
        } else {
            match summarize_embedded(&extracted).await {
                Ok(Some(s)) => {
                    let count = s.lines().filter(|l| l.trim().starts_with("- **")).count();
                    eprintln!("{} insight(s)", count);
                    s
                }
                Ok(None) => {
                    eprintln!("no insights");
                    continue; // no insights — skip file
                }
                Err(_) => {
                    eprintln!("error, using fallback");
                    fallback_summary(&extracted)
                }
            }
        };

        let safe_name = if args.session.is_some() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("claude-session")
                .to_string()
        } else {
            safe_rel_name(&claude_dir, &path)
        };
        let out_file = run_dir.join(format!("{safe_name}.summary.md"));
        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &out_file,
            format!(
                "# Summary\n\n{}\n\n## Source\n- `{}`\n",
                summary.trim(),
                path.display()
            ),
        )?;

        index.push(ExportedItem {
            source_path: path.to_string_lossy().to_string(),
            output_path: out_file.to_string_lossy().to_string(),
            chars_in: extracted.len(),
        });
    }

    let index_path = run_dir.join("index.json");
    fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;

    println!(
        "Exported {} Claude Code session(s) to {}",
        index.len(),
        run_dir.display()
    );
    Ok(())
}

/// Export and summarize specific Claude Code session ids for a project.
///
/// `project_dir_name` is the encoded project directory name under `~/.claude/projects/`
/// (e.g., `-Users-alice-dev-foo`).  If `session_ids` is empty all sessions are exported.
pub async fn export_claude_code_project_sessions(
    claude_dir: &Path,
    project_dir_name: &str,
    session_ids: &[String],
    out_dir: &Path,
) -> Result<usize> {
    let project_root = claude_dir.join("projects").join(project_dir_name);
    if !project_root.exists() {
        return Ok(0);
    }

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let session_paths: Vec<PathBuf> = if session_ids.is_empty() {
        discover_sessions_under(&project_root)?
    } else {
        let mut v = Vec::new();
        for raw_id in session_ids {
            let id = raw_id.trim();
            if id.is_empty() {
                continue;
            }
            let file_name = if id.ends_with(".jsonl") {
                id.to_string()
            } else {
                format!("{id}.jsonl")
            };
            let path = project_root.join(file_name);
            if !path.exists() {
                anyhow::bail!("Claude Code session not found: {}", path.display());
            }
            v.push(path);
        }
        v
    };

    let mut index: Vec<ExportedItem> = Vec::new();
    let total = session_paths.len();

    if total == 0 {
        let index_path = run_dir.join("index.json");
        fs::write(&index_path, "[]")?;
        return Ok(0);
    }

    eprintln!();
    eprintln!("  Summarizing {} session(s)...", total);

    for (i, path) in session_paths.iter().enumerate() {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("session");
        eprint!("  [{}/{}] {}... ", i + 1, total, name);
        let _ = io::stderr().flush();

        let raw = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
        let extracted = extract_text_from_jsonl(&raw);

        let summary = match summarize_embedded(&extracted)
            .await
            .with_context(|| format!("Summarization failed for {}", path.display()))?
        {
            Some(s) => {
                let count = s.lines().filter(|l| l.trim().starts_with("- **")).count();
                eprintln!("{} insight(s)", count);
                for line in s.lines() {
                    let t = line.trim();
                    if t.starts_with("- **") {
                        eprintln!("      {}", t);
                    }
                }
                s
            }
            None => {
                eprintln!("no insights");
                continue; // no insights — skip file
            }
        };

        let safe_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("claude-session")
            .to_string();
        let out_file = run_dir.join(format!("{safe_name}.summary.md"));

        fs::write(
            &out_file,
            format!(
                "# Summary\n\n{}\n\n## Source\n- `{}`\n",
                summary.trim(),
                path.display()
            ),
        )?;

        index.push(ExportedItem {
            source_path: path.to_string_lossy().to_string(),
            output_path: out_file.to_string_lossy().to_string(),
            chars_in: extracted.len(),
        });
    }

    let index_path = run_dir.join("index.json");
    fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
    Ok(index.len())
}

/// Derive the Claude Code project directory name from an absolute path.
///
/// Claude Code encodes the project path by replacing every path separator with `-`:
/// `/Users/alice/dev/foo` → `-Users-alice-dev-foo`
pub fn claude_code_project_dir_name(path: &Path) -> String {
    let s = path.to_string_lossy();
    // On Windows, also replace backslashes.
    s.replace('\\', "/").replace('/', "-")
}

fn discover_claude_code_sessions(claude_dir: &Path) -> Result<Vec<PathBuf>> {
    let projects_root = claude_dir.join("projects");
    if !projects_root.exists() {
        return Ok(vec![]);
    }

    let mut found = Vec::new();
    for entry in WalkDir::new(&projects_root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension() != Some(OsStr::new("jsonl")) {
            continue;
        }
        found.push(entry.into_path());
    }

    found.sort();
    found.dedup();
    Ok(found)
}

fn discover_sessions_under(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension() != Some(OsStr::new("jsonl")) {
            continue;
        }
        found.push(entry.into_path());
    }
    found.sort();
    found.dedup();
    Ok(found)
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
