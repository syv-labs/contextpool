use anyhow::{Context, Result};
use serde::Deserialize;

fn redact_secrets(text: &str) -> String {
    // Keep this intentionally simple and conservative: prefer false-positives over leaking secrets.
    // We redact common patterns found in terminal transcripts and env exports.
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let l = line.trim_end();

        // export FOO=... / export FOO="..." / export FOO='...'
        if let Some(rest) = l.strip_prefix("export ") {
            if let Some((k, _v)) = rest.split_once('=') {
                let key = k.trim();
                if key.ends_with("_KEY")
                    || key.ends_with("_TOKEN")
                    || key.ends_with("_SECRET")
                    || key.contains("API_KEY")
                    || key.contains("TOKEN")
                    || key.contains("SECRET")
                {
                    out.push_str(&format!("export {}=[REDACTED]\n", key));
                    continue;
                }
            }
        }

        // Inline env assignments: FOO=... or FOO="..." (common in multi-line command invocations)
        if let Some((k, _v)) = l.split_once('=') {
            let key = k.trim();
            if !key.contains(' ') // avoid catching arbitrary log lines
                && (key.ends_with("_KEY")
                    || key.ends_with("_TOKEN")
                    || key.ends_with("_SECRET")
                    || key.contains("API_KEY")
                    || key.contains("TOKEN")
                    || key.contains("SECRET"))
            {
                out.push_str(&format!("{}=[REDACTED]\n", key));
                continue;
            }
        }

        out.push_str(l);
        out.push('\n');
    }
    out
}

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

#[derive(Deserialize)]
struct ContextItem {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    file: Option<String>,
}

pub async fn summarize_via_api(text: &str, api_base: Option<&str>, api_key: Option<&str>) -> Result<String> {
    let api_base = api_base.context("Missing API base URL (set --api-base or CONTEXT_POOL_API_BASE)")?;

    // We summarize via the local context-generator-agent:
    // POST <api_base>/generate-context
    // Body: { chat: "<raw transcript text>", files: [], repo_type: "" }
    // Response: JSON array of 0-5 context items.
    let url = format!("{}/generate-context", api_base.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let redacted = redact_secrets(text);
    let body = serde_json::json!({
        "chat": redacted,
        "files": [],
        "repo_type": "",
    });

    let mut req = client.post(url).json(&body);
    if let Some(k) = api_key {
        if !k.trim().is_empty() {
            req = req.bearer_auth(k);
        }
    }
    let resp = req.send().await.context("API request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("API returned {}: {}", status, t);
    }

    let items = resp
        .json::<Vec<ContextItem>>()
        .await
        .context("Invalid API response JSON")?;

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
    Ok(out.trim().to_string())
}

