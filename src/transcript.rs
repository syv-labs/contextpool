/// Extract a clean conversation transcript from a JSONL file.
///
/// Supports two schemas:
/// - Claude Code  (`type` field at top level: "user" | "assistant" | ...)
/// - Cursor agent (`role` field at top level: "user" | "assistant")
///
/// For both schemas only human-readable text turns are kept.
/// Thinking blocks, tool calls/results, file snapshots, progress events, etc. are dropped.
/// Maximum extracted text size (~100KB). Transcripts beyond this are truncated.
const MAX_EXTRACTED_BYTES: usize = 100_000;

pub fn extract_text_from_jsonl(jsonl: &str) -> String {
    let mut out = String::new();
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Try Codex first: its lines always have a `payload` object and a `timestamp`
        // field, which distinguishes them from Claude Code / Cursor formats. If Codex
        // isn't tried first, the Claude Code parser's catch-all swallows Codex lines
        // (both use a top-level `type` field).
        if let Some(text) = try_codex(&v) {
            out.push_str(&text);
        } else if let Some(text) = try_claude_code(&v) {
            out.push_str(&text);
        } else if let Some(text) = try_cursor(&v) {
            out.push_str(&text);
        }
        else if let Some(text) = try_kiro(&v) {
            out.push_str(&text);
        }
        // Unknown format: skip entirely rather than dumping raw JSON noise

        // Cap to prevent memory issues with very large sessions
        if out.len() > MAX_EXTRACTED_BYTES {
            out.truncate(MAX_EXTRACTED_BYTES);
            out.push_str("\n\n[transcript truncated]\n");
            break;
        }
    }
    out
}

// ── Claude Code schema ────────────────────────────────────────────────────────

/// Claude Code JSONL line: top-level `type` is "user" or "assistant".
/// All other types (file-history-snapshot, progress, queue-operation, …) are ignored.
fn try_claude_code(v: &serde_json::Value) -> Option<String> {
    let record_type = v.get("type")?.as_str()?;
    let role = match record_type {
        "user" => "User",
        "assistant" => "Assistant",
        _ => return Some(String::new()), // known-but-unneeded type: swallow silently
    };

    let message = v.get("message")?;
    let content = message.get("content")?;

    let text = match content {
        // Simple string content (rare but valid)
        serde_json::Value::String(s) => {
            let t = s.trim().to_string();
            if t.is_empty() { return Some(String::new()); }
            t
        }
        // Array of typed content blocks
        serde_json::Value::Array(blocks) => {
            let mut parts: Vec<&str> = Vec::new();
            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    // Keep only plain text blocks
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            let t = t.trim();
                            if !t.is_empty() {
                                parts.push(t);
                            }
                        }
                    }
                    // Drop everything else: thinking, tool_use, tool_result, …
                    _ => {}
                }
            }
            if parts.is_empty() { return Some(String::new()); }
            parts.join("\n")
        }
        _ => return Some(String::new()),
    };

    Some(format!("{role}: {text}\n\n"))
}

// ── Cursor agent schema ───────────────────────────────────────────────────────

/// Cursor JSONL line: top-level `role` is "user" or "assistant",
/// `message` is an object with a `content` array of `{type, text}` blocks.
fn try_cursor(v: &serde_json::Value) -> Option<String> {
    let role = match v.get("role")?.as_str()? {
        "user" => "User",
        "assistant" => "Assistant",
        _ => return None,
    };

    let message = v.get("message")?;

    // content may be a direct string or an array of blocks
    let text = if let Some(s) = message.as_str() {
        s.trim().to_string()
    } else if let Some(content) = message.get("content") {
        match content {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Array(blocks) => {
                let mut parts: Vec<&str> = Vec::new();
                for block in blocks {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("text");
                    if block_type == "text" {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            let t = t.trim();
                            if !t.is_empty() {
                                parts.push(t);
                            }
                        }
                    }
                }
                parts.join("\n")
            }
            _ => return None,
        }
    } else {
        return None;
    };

    if text.is_empty() {
        return Some(String::new());
    }

    Some(format!("{role}: {text}\n\n"))
}

// ── Codex CLI schema ─────────────────────────────────────────────────────────

/// Codex JSONL line: top-level `type` is one of:
/// - `"event_msg"` with `payload.type` = `"user_message"` or `"agent_message"`
/// - `"session_meta"`, `"response_item"`, `"turn_context"` (all skipped)
///
/// We extract text exclusively from `event_msg` records to avoid duplicating
/// content that also appears in `response_item/message` lines and to skip
/// developer system prompts entirely.
fn try_codex(v: &serde_json::Value) -> Option<String> {
    // Guard: Codex lines always carry both `timestamp` and `payload` at the top level.
    // This distinguishes them from Claude Code lines (which have `type` + `message`)
    // and Cursor lines (which have `role` + `message`).
    if v.get("timestamp").is_none() || v.get("payload").is_none() {
        return None;
    }

    let record_type = v.get("type")?.as_str()?;

    match record_type {
        "event_msg" => {
            let payload = v.get("payload")?;
            let event_type = payload.get("type")?.as_str()?;
            match event_type {
                "user_message" => {
                    let msg = payload.get("message")?.as_str()?.trim();
                    if msg.is_empty() {
                        return Some(String::new());
                    }
                    Some(format!("User: {msg}\n\n"))
                }
                "agent_message" => {
                    let msg = payload.get("message")?.as_str()?.trim();
                    if msg.is_empty() {
                        return Some(String::new());
                    }
                    Some(format!("Assistant: {msg}\n\n"))
                }
                // Known-but-unneeded event types: swallow silently
                _ => Some(String::new()),
            }
        }
        // Known Codex record types that carry no user/assistant text
        "session_meta" | "response_item" | "turn_context" => Some(String::new()),
        _ => None,
    }
}

/// Read the `cwd` field from the first `session_meta` line of a Codex JSONL file.
///
/// Returns `None` if the file cannot be read or doesn't have a `session_meta` record.
pub fn extract_codex_cwd(path: &std::path::Path) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().take(5) {
        let line = line.ok()?;
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&line).ok()?;
        if v.get("type")?.as_str()? == "session_meta" {
            return v
                .get("payload")?
                .get("cwd")?
                .as_str()
                .map(|s| s.to_string());
        }
    }
    None
}

/// Kiro JSONL line: top-level `kind` is "Prompt" or "AssistantMessage".
/// `data` is an object with a `content` array of `{kind, data}` blocks.
/// We extract the "text" kinds and ignore "ToolResults", "toolUse", etc.
fn try_kiro(v: &serde_json::Value) -> Option<String> {
    if v.get("data").is_none() {                                                                                                     
        return None;                                                                                                                 
    }
    let kind = v.get("kind")?.as_str()?;
    
    let role = match kind {
        "Prompt" => "User",
        "AssistantMessage" => "Assistant",
        // Silently swallow other events like "ToolResults"
        _ => return Some(String::new()), 
    };

    let data = v.get("data")?;
    let content = data.get("content")?;

    let text = match content {
        serde_json::Value::Array(blocks) => {
            let mut parts: Vec<&str> = Vec::new();
            for block in blocks {
                let block_kind = block.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                match block_kind {
                    // Keep only plain text blocks, the text itself is stored in `data`
                    "text" => {
                        if let Some(t) = block.get("data").and_then(|d| d.as_str()) {
                            let t = t.trim();
                            if !t.is_empty() {
                                parts.push(t);
                            }
                        }
                    }
                    // Drop everything else: toolUse, toolResult, etc.
                    _ => {}
                }
            }
            if parts.is_empty() { 
                return Some(String::new()); 
            }
            parts.join("\n")
        }
        _ => return Some(String::new()),
    };

    Some(format!("{role}: {text}\n\n"))
}

// ── Legacy helpers (used by export/vscdb and export/kiro paths) ───────────────

pub fn extract_text_from_json(json: &str) -> String {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => extract_strings_deep(&v).join("\n"),
        Err(_) => trimmed.to_string(),
    }
}

fn extract_strings_deep(v: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_value(v, &mut out);
    out
}

fn walk_value(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
        serde_json::Value::String(s) => {
            let t = s.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
        serde_json::Value::Array(arr) => {
            for x in arr {
                walk_value(x, out);
            }
        }
        serde_json::Value::Object(obj) => {
            for k in ["content", "text", "message", "body"] {
                if let Some(val) = obj.get(k) {
                    if let Some(s) = val.as_str() {
                        let t = s.trim();
                        if !t.is_empty() {
                            out.push(t.to_string());
                        }
                    }
                }
            }
            for (_k, val) in obj {
                walk_value(val, out);
            }
        }
    }
}
