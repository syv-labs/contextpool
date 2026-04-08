use crate::{
    cli::ExportCursorArgs,
    paths::{default_cursor_dir, default_out_dir},
    summarize::{fallback_summary, summarize_embedded},
    transcript::extract_text_from_jsonl,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub async fn export_cursor(args: ExportCursorArgs) -> Result<()> {
    let cursor_dir = args
        .cursor_dir
        .or_else(|| default_cursor_dir())
        .context("Could not determine Cursor directory (try --cursor-dir)")?;

    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join("exports").join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let transcript_paths = if let Some(single) = args.transcript.clone() {
        vec![single]
    } else {
        discover_cursor_transcripts(&cursor_dir)?
    };
    let mut index: Vec<ExportedItem> = Vec::new();

    for path in transcript_paths {
        let raw = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
        let extracted = extract_text_from_jsonl(&raw);

        let summary = if args.offline {
            fallback_summary(&extracted)
        } else {
            match summarize_embedded(&extracted).await {
                Ok(Some(s)) => s,
                Ok(None) => continue, // no insights — skip file
                Err(_) => fallback_summary(&extracted),
            }
        };

        let safe_name = if args.transcript.is_some() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("cursor-transcript")
                .to_string()
        } else {
            safe_rel_name(&cursor_dir, &path)
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
        "Exported {} transcript(s) to {}",
        index.len(),
        run_dir.display()
    );
    Ok(())
}

pub struct CursorExportOpts {
    pub out_dir: PathBuf,
    pub offline: bool,
}

/// Export transcripts for a specific Cursor project id into a centralized folder.
pub async fn export_cursor_project(cursor_dir: &Path, project_id: &str, opts: CursorExportOpts) -> Result<usize> {
    let project_root = cursor_dir.join("projects").join(project_id).join("agent-transcripts");
    if !project_root.exists() {
        // Nothing to import yet; user may not have opened the project in Cursor.
        return Ok(0);
    }

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = opts
        .out_dir
        .join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let transcript_paths = discover_transcripts_under(&project_root)?;
    let mut index: Vec<ExportedItem> = Vec::new();

    for path in transcript_paths {
        let raw = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
        let extracted = extract_text_from_jsonl(&raw);

        let summary = if opts.offline {
            fallback_summary(&extracted)
        } else {
            match summarize_embedded(&extracted).await {
                Ok(Some(s)) => s,
                Ok(None) => continue, // no insights — skip file
                Err(_) => fallback_summary(&extracted),
            }
        };

        let safe_name = safe_rel_name(&project_root, &path);
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

    Ok(index.len())
}

/// Export and summarize specific Cursor transcript ids for a project.
///
/// This is the "init flow": you pass chat ids, we only read those files, call the API,
/// and store only the resulting summaries + index in `out_dir/<timestamp>/`.
pub async fn export_cursor_project_chats(
    cursor_dir: &Path,
    project_id: &str,
    chat_ids: &[String],
    out_dir: &Path,
) -> Result<usize> {
    let project_root = cursor_dir.join("projects").join(project_id).join("agent-transcripts");
    if !project_root.exists() {
        return Ok(0);
    }

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let mut index: Vec<ExportedItem> = Vec::new();

    let transcript_paths: Vec<PathBuf> = if chat_ids.is_empty() {
        discover_transcripts_under(&project_root)?
    } else {
        let mut v = Vec::new();
        for raw_id in chat_ids {
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
                anyhow::bail!("Cursor transcript not found: {}", path.display());
            }
            v.push(path);
        }
        v
    };

    for path in transcript_paths {

        let raw = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
        let extracted = extract_text_from_jsonl(&raw);

        let summary = match summarize_embedded(&extracted)
            .await
            .with_context(|| format!("Summarization failed for {}", path.display()))?
        {
            Some(s) => s,
            None => continue, // no insights — skip file
        };

        let safe_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cursor-transcript")
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
fn discover_cursor_transcripts(cursor_dir: &Path) -> Result<Vec<PathBuf>> {
    let candidates = [
        cursor_dir.join("agent-transcripts"),
        cursor_dir.join("projects"),
    ];

    let mut found = Vec::new();
    for base in candidates {
        if !base.exists() {
            continue;
        }

        for entry in WalkDir::new(&base).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension() != Some(OsStr::new("jsonl")) {
                continue;
            }
            if !entry
                .path()
                .components()
                .any(|c| c.as_os_str() == OsStr::new("agent-transcripts"))
            {
                continue;
            }
            found.push(entry.into_path());
        }
    }

    found.sort();
    found.dedup();
    Ok(found)
}
fn discover_transcripts_under(root: &Path) -> Result<Vec<PathBuf>> {
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

