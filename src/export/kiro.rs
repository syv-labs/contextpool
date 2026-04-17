use crate::{
    cli::ExportKiroArgs,
    paths::default_out_dir,
    summarize::{fallback_summary, summarize_embedded},
    transcript::{extract_text_from_jsonl},
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}};

pub async fn export_kiro(args: ExportKiroArgs) -> Result<()> {
    let out_dir = args.out.unwrap_or_else(|| default_out_dir());
    fs::create_dir_all(&out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join("exports").join("kiro").join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let src = args.chat_json;
    let raw = fs::read_to_string(&src).with_context(|| format!("Reading {}", src.display()))?;
    let extracted = extract_text_from_jsonl(&raw);

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

pub async fn export_kiro_project_sessions(
    kiro_dir: &Path,
    cwd: &Path,
    session_ids: &[String],
    out_dir: &Path,
) -> Result<usize> {
    let session_root = kiro_dir.join("sessions").join("cli");
    if !session_root.exists() {
        return Ok(0);
    }

    let run_id = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let run_dir = out_dir.join(run_id.replace(':', "-"));
    fs::create_dir_all(&run_dir).with_context(|| format!("Creating {}", run_dir.display()))?;

    let session_paths: Vec<PathBuf> = if session_ids.is_empty() {
        // Assumes a helper that checks the companion .json files to ensure
        // the session belongs to the current `cwd`
        discover_kiro_sessions_under(&session_root, cwd)?
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
            let path = session_root.join(file_name);
            if !path.exists() {
                anyhow::bail!("Kiro session not found: {}", path.display());
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
            .unwrap_or("kiro-session")
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

fn discover_kiro_sessions_under(session_root: &Path, cwd: &Path) -> Result<Vec<PathBuf>> {                                           
      if !session_root.exists() {                                                                                                      
          return Ok(vec![]);                                                                                                           
      }                                                                                                                                
                                                                                                                                       
      let cwd_str = cwd.to_string_lossy();                                                                                             
      let mut found = Vec::new();                                                                                                      
                                                                                                                                       
      for entry in std::fs::read_dir(session_root)? {
          let entry = entry?;                                                                                                          
          let path = entry.path();                                                                                                   

          // Only look at .json metadata files (not .jsonl or .lock)                                                                   
          if path.extension().and_then(|e| e.to_str()) != Some("json") {
              continue;                                                                                                                
          }                                                                                                                          

          // Parse the metadata to check the cwd                                                                                       
          let raw = match std::fs::read_to_string(&path) {
              Ok(s) => s,                                                                                                              
              Err(_) => continue,                                                                                                    
          };                                                                                                                           
          let meta: serde_json::Value = match serde_json::from_str(&raw) {                                                           
              Ok(v) => v,                                                                                                              
              Err(_) => continue,
          };                                                                                                                           
                                                                                                                                     
          let session_cwd = match meta.get("cwd").and_then(|v| v.as_str()) {                                                           
              Some(s) => s.to_string(),
              None => continue,                                                                                                        
          };                                                                                                                         

          if session_cwd != cwd_str.as_ref() {                                                                                         
              continue;
          }                                                                                                                            
                                                                                                                                     
          // Found a matching session — add the companion .jsonl                                                                       
          let jsonl_path = path.with_extension("jsonl");
          if jsonl_path.exists() {                                                                                                     
              found.push(jsonl_path);                                                                                                  
          }
      }                                                                                                                                
                                                                                                                                     
      found.sort();
      Ok(found)
  }

#[derive(serde::Serialize)]
struct ExportedItem {
    source_path: String,
    output_path: String,
    chars_in: usize,
}

