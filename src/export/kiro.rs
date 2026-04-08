use crate::{
    cli::ExportKiroArgs,
    paths::default_out_dir,
    summarize::{fallback_summary, summarize_embedded},
    transcript::extract_text_from_json,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::fs;

pub async fn export_kiro(args: ExportKiroArgs) -> Result<()> {
    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join("exports").join("kiro").join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let src = args.chat_json;
    let raw = fs::read_to_string(&src).with_context(|| format!("Reading {}", src.display()))?;
    let extracted = extract_text_from_json(&raw);

    let summary = if args.offline {
        fallback_summary(&extracted)
    } else {
        match summarize_embedded(&extracted).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("No insights extracted from Kiro chat — skipping.");
                return Ok(());
            }
            Err(_) => fallback_summary(&extracted),
        }
    };

    let out_file = run_dir.join("kiro-chat.summary.md");
    fs::write(
        &out_file,
        format!("# Summary\n\n{}\n\n## Source\n- `{}`\n", summary.trim(), src.display()),
    )?;

    let index_path = run_dir.join("index.json");
    let index = vec![ExportedItem {
        source_path: src.to_string_lossy().to_string(),
        output_path: out_file.to_string_lossy().to_string(),
        chars_in: extracted.len(),
    }];
    fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;

    println!("Exported 1 Kiro chat export to {}", run_dir.display());
    Ok(())
}

#[derive(serde::Serialize)]
struct ExportedItem {
    source_path: String,
    output_path: String,
    chars_in: usize,
}

