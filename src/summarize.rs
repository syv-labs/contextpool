use anyhow::Result;
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use crate::{credentials::{ensure_nvidia_api_key_interactive, load_api_backend, ApiBackend}, embedded_agent, redact::redact_secrets};

pub fn fallback_summary(text: &str) -> String {
    let redacted = redact_secrets(text);
    let trimmed = redacted.trim();
    if trimmed.is_empty() {
        return "No extractable text found in transcript.".to_string();
    }
    format!(
        "Offline summary (no API configured). Extracted {} chars.\n\n(Content not stored. Configure CONTEXT_POOL_API_BASE to summarize via context-generator-agent.)",
        trimmed.len()
    )
}

pub async fn summarize_embedded(text: &str) -> Result<String> {
    // Prefer Anthropic key (already in env when running inside Claude Code / Cursor),
    // fall back to NVIDIA with an interactive prompt.
    let backend = match load_api_backend() {
        Some(b) => b,
        None => ApiBackend::Nvidia(ensure_nvidia_api_key_interactive()?),
    };

    let redacted = redact_secrets(text);
    let opts = embedded_agent::EmbeddedAgentOptions::from_env(backend);

    // CLI feedback: show progress while the model is generating.
    // (We avoid adding a spinner dependency by printing dots from a background thread.)
    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = stop.clone();
    let label = "Generating summary";
    let bg = thread::spawn(move || {
        let mut dots: usize = 0;
        while !stop_bg.load(Ordering::SeqCst) {
            dots = (dots + 1) % 4;
            let trail = ".".repeat(dots + 1);
            let _ = write!(
                io::stderr(),
                "\r{}{}{}",
                label,
                trail,
                "   " // erase remnants
            );
            let _ = io::stderr().flush();
            thread::sleep(Duration::from_millis(180));
        }

        // Clear the line so the next log/output starts cleanly.
        let _ = write!(io::stderr(), "\r{}... done\n", label);
        let _ = io::stderr().flush();
    });

    let (items, debug_raw) = {
        let res = embedded_agent::generate_context_items(&redacted, &[], "", &opts).await;
        stop.store(true, Ordering::SeqCst);
        res
    }?;

    // Best-effort join: if the indicator thread is already done, joining is quick.
    let _ = bg.join();

    if items.is_empty() {
        return Ok("No high-signal engineering insights extracted.".to_string());
    }

    let mut out = String::new();
    out.push_str("## Extracted insights\n\n");
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
        if let Some(f) = it.file.as_deref() {
            let f = f.trim();
            if !f.is_empty() {
                out.push_str(&format!("  - file: `{}`\n", f));
            }
        }
        if !it.tags.is_empty() {
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
    }

    if let Some(raw) = debug_raw {
        out.push_str("\n\n---\n\n## Debug (raw model output)\n\n");
        out.push_str(raw.trim());
        out.push('\n');
    }

    Ok(out.trim().to_string())
}

