use crate::{
    cli::ExportCodexArgs,
    paths::{default_codex_dir, default_out_dir},
    summarize::{fallback_summary, summarize_embedded},
    transcript::{extract_codex_cwd, extract_text_from_jsonl},
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

pub async fn export_codex(args: ExportCodexArgs) -> Result<()> {
    let codex_dir = args
        .codex_dir
        .or_else(|| default_codex_dir())
        .context("Could not determine Codex directory (try --codex-dir or set $CODEX_HOME)")?;

    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join("exports").join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let session_paths = if let Some(single) = args.session.clone() {
        vec![single]
    } else {
        discover_all_codex_sessions(&codex_dir)?
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
                    continue;
                }
                Err(_) => {
                    eprintln!("error, using fallback");
                    fallback_summary(&extracted)
                }
            }
        };

        let safe_name = safe_rel_name(&codex_dir, path);
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
        "Exported {} Codex session(s) to {}",
        index.len(),
        run_dir.display()
    );
    Ok(())
}

/// Export and summarize Codex sessions whose `cwd` matches a given project path.
///
/// If `session_ids` is empty, all sessions matching the project path are exported.
/// Session IDs correspond to the UUID portion of the rollout file name.
pub async fn export_codex_project_sessions(
    codex_dir: &Path,
    project_path: &Path,
    session_ids: &[String],
    out_dir: &Path,
) -> Result<usize> {
    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let session_paths: Vec<PathBuf> = if session_ids.is_empty() {
        discover_codex_sessions_for_project(codex_dir, project_path)?
    } else {
        let all = discover_all_codex_sessions(codex_dir)?;
        let mut matched = Vec::new();
        for raw_id in session_ids {
            let id = raw_id.trim();
            if id.is_empty() {
                continue;
            }
            let found = all.iter().find(|p| {
                p.to_string_lossy().contains(id)
            });
            match found {
                Some(p) => matched.push(p.clone()),
                None => anyhow::bail!("Codex session not found for id: {id}"),
            }
        }
        matched
    };

    let mut index: Vec<ExportedItem> = Vec::new();
    let total = session_paths.len();

    if total == 0 {
        let index_path = run_dir.join("index.json");
        fs::write(&index_path, "[]")?;
        return Ok(0);
    }

    eprintln!();
    eprintln!("  Summarizing {} Codex session(s)...", total);

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
                continue;
            }
        };

        let safe_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("codex-session")
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

/// Discover all Codex session files under `~/.codex/sessions/`.
pub fn discover_all_codex_sessions(codex_dir: &Path) -> Result<Vec<PathBuf>> {
    let sessions_root = codex_dir.join("sessions");
    if !sessions_root.exists() {
        return Ok(vec![]);
    }

    let mut found = Vec::new();
    for entry in WalkDir::new(&sessions_root).follow_links(false) {
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

/// Normalize a path lexically (resolve `.` and `..` components) without any
/// filesystem access, avoiding macOS TCC permission prompts for protected
/// directories (Documents, Downloads, etc.).
fn normalize_path_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// Discover Codex sessions whose `cwd` matches (or is a child of) `project_path`.
pub fn discover_codex_sessions_for_project(
    codex_dir: &Path,
    project_path: &Path,
) -> Result<Vec<PathBuf>> {
    let all = discover_all_codex_sessions(codex_dir)?;
    // Canonicalize the project path once (it's a known accessible path).
    // Also keep a lexically-normalized version for comparison against session
    // cwd values that may not have had symlinks resolved (e.g. /var vs
    // /private/var on macOS).
    let canonical_project = std::fs::canonicalize(project_path)
        .unwrap_or_else(|_| normalize_path_lexical(project_path));
    let lexical_project = normalize_path_lexical(project_path);

    let mut matched = Vec::new();
    for path in all {
        if let Some(cwd) = extract_codex_cwd(&path) {
            let cwd_path = PathBuf::from(&cwd);
            // Use lexical normalization only — avoids filesystem access on
            // arbitrary paths from session metadata, which would trigger macOS
            // TCC permission prompts for protected directories.
            let normalized_cwd = normalize_path_lexical(&cwd_path);
            if normalized_cwd.starts_with(&canonical_project)
                || normalized_cwd.starts_with(&lexical_project)
            {
                matched.push(path);
            }
        }
    }
    Ok(matched)
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
