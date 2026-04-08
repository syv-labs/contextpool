use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::credentials::ApiBackend;

const NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";

// Mirrors `context-generator-agent/main.py` intent.
const SYSTEM_PROMPT: &str = r#"
You are a senior software engineer extracting high-signal engineering memory from developer chats.

Your task is NOT to summarize the conversation.
Your task is to DISTILL reusable engineering insights, like writing precise commit notes.

CORE PRINCIPLE:
- Think like a senior engineer documenting decisions, bugs, and fixes for future developers.

PROCESS (perform internally, do not output steps):
1. Identify:
   - bugs encountered
   - root causes
   - fixes or solutions
   - design decisions
   - non-obvious patterns or gotchas

2. Filter:
   - REMOVE generic explanations
   - REMOVE repeated ideas
   - REMOVE vague or incomplete thoughts
   - KEEP only actionable, specific insights tied to code or decisions

3. Prioritize:
   - bugs that caused failures
   - root causes and fixes
   - architectural or design decisions
   - non-obvious insights

4. Deprioritize:
   - basic programming explanations
   - obvious fixes
   - exploratory discussion
   - conversational fluff

5. Compress:
   - Each insight MUST be <= 400 characters

OUTPUT REQUIREMENTS:
- Max 10 objects
- If no strong insights exist, return []
- Output must be a JSON ARRAY (list). Not an object/dict.
- Do not wrap the JSON in Markdown fences (no ```).
- Each object must match this schema exactly: {type, title, summary, tags, file?}
- Allowed keys per object: type, title, summary, tags, file. No other keys.
- If you are about to output anything other than a JSON array of objects with the allowed keys, output [] instead.

TAGS REQUIREMENTS:
- Each insight MUST have 5-10 tags; add more if the insight warrants it
- Tags should cover: language, framework, library, component/module, error type, concept, pattern, file type
- Use specific tags (e.g. "tokio-runtime", "lifetime-borrow") not generic ones (e.g. "rust", "error")
- Include both broad category tags and narrow specific tags

EXAMPLE (shape only; content will differ):
[
  {
    "type": "bug",
    "title": "Fix ESM vs CJS mismatch",
    "summary": "Add \"type\": \"module\" with NodeNext to resolve verbatimModuleSyntax import/export errors.",
    "tags": ["typescript", "esm", "commonjs", "tsconfig", "module-resolution", "nodejs", "verbatimModuleSyntax", "import-export"],
    "file": "client.ts"
  }
]
"#;

fn user_prompt(chat_text: &str, files: &[String], repo_type: &str) -> String {
    let files_joined = if files.is_empty() {
        "".to_string()
    } else {
        files.join(", ")
    };
    format!(
        r#"
CHAT TRANSCRIPT:
{chat_text}

FILES:
{files}

REPO TYPE:
{repo_type}

Return STRICT JSON only.
"#,
        chat_text = chat_text,
        files = files_joined,
        repo_type = repo_type
    )
}

#[derive(Debug, Clone)]
pub struct EmbeddedAgentOptions {
    pub backend: ApiBackend,
    pub model: String,
    pub repair_model: String,
    pub temperature: f32,
    pub top_p: f32,
    pub max_completion_tokens: u32,
    pub sanitize_chat: bool,
    pub extract_user_queries_only: bool,
    pub debug_raw_output: bool,
}

impl EmbeddedAgentOptions {
    pub fn from_env(backend: ApiBackend) -> Self {
        let default_model = match &backend {
            ApiBackend::ClaudeCodeCli => ANTHROPIC_DEFAULT_MODEL.to_string(),
            ApiBackend::Anthropic(_) => ANTHROPIC_DEFAULT_MODEL.to_string(),
            ApiBackend::OpenAI(_) => OPENAI_DEFAULT_MODEL.to_string(),
            ApiBackend::Nvidia(_) => "qwen/qwen3.5-122b-a10b".to_string(),
        };
        let model = std::env::var("MODEL").unwrap_or(default_model);
        let repair_model = std::env::var("REPAIR_MODEL").unwrap_or_else(|_| model.clone());
        let temperature = std::env::var("TEMPERATURE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        let top_p = std::env::var("TOP_P")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.95);
        let max_completion_tokens = std::env::var("MAX_COMPLETION_TOKENS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(4096);

        let sanitize_chat = std::env::var("SANITIZE_CHAT")
            .unwrap_or_else(|_| "1".to_string())
            .to_lowercase();
        let sanitize_chat = matches!(sanitize_chat.as_str(), "1" | "true" | "yes" | "on");

        let extract_user_queries_only = std::env::var("EXTRACT_USER_QUERIES_ONLY")
            .unwrap_or_else(|_| "0".to_string())
            .to_lowercase();
        let extract_user_queries_only =
            matches!(extract_user_queries_only.as_str(), "1" | "true" | "yes" | "on");

        let debug_raw_output = std::env::var("DEBUG_LLM_OUTPUT")
            .unwrap_or_default()
            .to_lowercase();
        let debug_raw_output = matches!(debug_raw_output.as_str(), "1" | "true" | "yes" | "on");

        Self {
            backend,
            model,
            repair_model,
            temperature,
            top_p,
            max_completion_tokens,
            sanitize_chat,
            extract_user_queries_only,
            debug_raw_output,
        }
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(rename = "max_tokens", skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

// ── Anthropic Messages API types ─────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ContextItem {
    #[serde(default, rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub file: Option<String>,
}

pub fn sanitize_chat_text(text: &str, extract_user_queries_only: bool) -> String {
    let mut t = text.to_string();

    if extract_user_queries_only && t.to_lowercase().contains("<user_query>") {
        let re = Regex::new(r"(?is)<user_query>\s*([\s\S]*?)\s*</user_query>").unwrap();
        let mut queries: Vec<String> = Vec::new();
        for cap in re.captures_iter(&t) {
            if let Some(m) = cap.get(1) {
                let q = m.as_str().trim();
                if !q.is_empty() {
                    queries.push(q.to_string());
                }
            }
        }
        if !queries.is_empty() {
            t = queries.join("\n\n");
        }
    }

    // Drop <think> blocks
    t = Regex::new(r"(?is)<think>[\s\S]*?</think>")
        .unwrap()
        .replace_all(&t, "")
        .to_string();

    // Drop common XML-ish blocks (injected context and editor artifacts)
    for pat in [
        r"(?is)<attached_files>[\s\S]*?</attached_files>",
        r"(?is)<code_selection[\s\S]*?</code_selection>",
        r"(?is)<terminal_selection[\s\S]*?</terminal_selection>",
        r"(?is)<ide_opened_file[\s\S]*?</ide_opened_file>",
        r"(?is)<environment_details[\s\S]*?</environment_details>",
        r"(?is)<system>[\s\S]*?</system>",
    ] {
        t = Regex::new(pat).unwrap().replace_all(&t, "").to_string();
    }

    // Remove tool call/result lines
    t = Regex::new(r"(?m)^\[Tool call\].*$")
        .unwrap()
        .replace_all(&t, "")
        .to_string();
    t = Regex::new(r"(?m)^\[Tool result\].*$")
        .unwrap()
        .replace_all(&t, "")
        .to_string();

    // Remove patch blobs
    t = Regex::new(r"(?is)\*\*\* Begin Patch[\s\S]*?\*\*\* End Patch")
        .unwrap()
        .replace_all(&t, "")
        .to_string();

    // Remove markdown fences but keep contents
    t = Regex::new(r"(?is)```(?:json|python|typescript|javascript|bash|sh|text)?\n")
        .unwrap()
        .replace_all(&t, "")
        .to_string();
    t = t.replace("```", "");

    // Drop lines that are likely raw file content (very long, no spaces typical of code dumps)
    t = t
        .lines()
        .filter(|l| l.len() <= 500)
        .collect::<Vec<_>>()
        .join("\n");

    // Collapse excessive blank lines
    t = Regex::new(r"\n{3,}").unwrap().replace_all(&t, "\n\n").to_string();

    t.trim().to_string()
}

fn extract_first_json_candidate(text: &str) -> Option<String> {
    let cleaned = text.trim();

    // Try each '[' from left to right, paired with the rightmost ']'.
    // This handles preamble text containing brackets before the actual JSON
    // (e.g. "Note: the list [here] ... [{json}]").
    if let Some(end) = cleaned.rfind(']') {
        let mut search_from = 0;
        while search_from <= end {
            if let Some(rel) = cleaned[search_from..=end].find('[') {
                let abs_start = search_from + rel;
                let candidate = &cleaned[abs_start..=end];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
                search_from = abs_start + 1;
            } else {
                break;
            }
        }
    }

    // Fallback: try JSON objects the same way.
    if let Some(end) = cleaned.rfind('}') {
        let mut search_from = 0;
        while search_from <= end {
            if let Some(rel) = cleaned[search_from..=end].find('{') {
                let abs_start = search_from + rel;
                let candidate = &cleaned[abs_start..=end];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
                search_from = abs_start + 1;
            } else {
                break;
            }
        }
    }

    None
}

fn parse_context_items(raw_text: &str) -> Vec<ContextItem> {
    let candidate = extract_first_json_candidate(raw_text).unwrap_or_else(|| raw_text.trim().to_string());
    let data: serde_json::Value = match serde_json::from_str(&candidate) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let arr_value = match data {
        serde_json::Value::Array(a) => serde_json::Value::Array(a),
        serde_json::Value::Object(map) => {
            // Common model failure: object keyed by filename/type → coerce values to list.
            serde_json::Value::Array(map.into_values().collect())
        }
        _ => serde_json::Value::Array(vec![]),
    };

    serde_json::from_value::<Vec<ContextItem>>(arr_value).unwrap_or_default()
}

async fn call_nvidia_chat(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
) -> Result<String> {
    let url = format!("{}/chat/completions", NVIDIA_BASE_URL.trim_end_matches('/'));
    let req_body = ChatCompletionRequest {
        model,
        messages: vec![
            Message {
                role: "system",
                content: system,
            },
            Message {
                role: "user",
                content: user,
            },
        ],
        temperature: Some(temperature),
        top_p: Some(top_p),
        max_tokens: Some(max_tokens),
        stream: false,
    };

    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&req_body)
        .send()
        .await
        .context("NVIDIA chat completion request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("NVIDIA API returned {}: {}", status, t);
    }

    let parsed = resp
        .json::<ChatCompletionResponse>()
        .await
        .context("Invalid NVIDIA chat completion JSON")?;

    let content = parsed
        .choices
        .get(0)
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();

    Ok(content)
}

async fn call_chat(
    client: &reqwest::Client,
    backend: &ApiBackend,
    model: &str,
    system: &str,
    user: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
) -> Result<String> {
    let result = match backend {
        ApiBackend::ClaudeCodeCli => call_claude_cli(system, user).await,
        ApiBackend::Anthropic(key) => {
            call_anthropic_chat(client, key, model, system, user, max_tokens).await
        }
        ApiBackend::OpenAI(key) => {
            call_openai_chat(client, key, model, system, user, temperature, top_p, max_tokens).await
        }
        ApiBackend::Nvidia(key) => {
            call_nvidia_chat(client, key, model, system, user, temperature, top_p, max_tokens).await
        }
    };

    // One retry on transient errors (429, 5xx, network)
    match result {
        Ok(v) => Ok(v),
        Err(e) if is_transient_error(&e) => {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match backend {
                ApiBackend::ClaudeCodeCli => call_claude_cli(system, user).await,
                ApiBackend::Anthropic(key) => {
                    call_anthropic_chat(client, key, model, system, user, max_tokens).await
                }
                ApiBackend::OpenAI(key) => {
                    call_openai_chat(client, key, model, system, user, temperature, top_p, max_tokens).await
                }
                ApiBackend::Nvidia(key) => {
                    call_nvidia_chat(client, key, model, system, user, temperature, top_p, max_tokens).await
                }
            }
        }
        Err(e) => Err(e),
    }
}

/// Check if an error looks transient (rate limit, server error, network).
fn is_transient_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    // HTTP status codes that are transient
    for code in ["429", "500", "502", "503", "504"] {
        if msg.contains(code) {
            return true;
        }
    }
    // Network errors
    msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("request failed")
}

async fn call_anthropic_chat(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String> {
    let url = format!("{}/messages", ANTHROPIC_BASE_URL);
    let req_body = AnthropicRequest {
        model,
        max_tokens,
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user,
        }],
    };

    let resp = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&req_body)
        .send()
        .await
        .context("Anthropic messages request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API returned {}: {}", status, t);
    }

    let parsed = resp
        .json::<AnthropicResponse>()
        .await
        .context("Invalid Anthropic messages JSON")?;

    let text = parsed
        .content
        .into_iter()
        .filter(|b| b.block_type == "text")
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}

async fn call_openai_chat(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
) -> Result<String> {
    let url = format!("{}/chat/completions", OPENAI_BASE_URL);
    let req_body = ChatCompletionRequest {
        model,
        messages: vec![
            Message { role: "system", content: system },
            Message { role: "user", content: user },
        ],
        temperature: Some(temperature),
        top_p: Some(top_p),
        max_tokens: Some(max_tokens),
        stream: false,
    };
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&req_body)
        .send()
        .await
        .context("OpenAI request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API returned {}: {}", status, t);
    }
    let parsed = resp
        .json::<ChatCompletionResponse>()
        .await
        .context("Invalid OpenAI JSON")?;
    Ok(parsed
        .choices
        .into_iter()
        .filter_map(|c| c.message.content)
        .collect::<Vec<_>>()
        .join(""))
}

async fn call_claude_cli(system: &str, user: &str) -> Result<String> {
    let combined = format!("{}\n\n{}", system.trim(), user.trim());
    let output = tokio::process::Command::new("claude")
        .arg("-p")
        .arg(&combined)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .context("Failed to spawn `claude` CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`claude -p` exited with {}: {}", output.status, stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub async fn generate_context_items(
    chat_text: &str,
    files: &[String],
    repo_type: &str,
    opts: &EmbeddedAgentOptions,
) -> Result<(Vec<ContextItem>, Option<String>)> {
    let mut chat = chat_text.to_string();
    if opts.sanitize_chat {
        chat = sanitize_chat_text(&chat, opts.extract_user_queries_only);
    }

    // Truncate very long transcripts to keep LLM latency and token cost bounded.
    // The LLM only needs the most signal-dense portion; later content is typically
    // repetitive or low-signal. Override with CXP_MAX_CHARS env var.
    let max_chars = std::env::var("CXP_MAX_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20_000);
    if chat.len() > max_chars {
        chat.truncate(max_chars);
        // Trim to the last newline so we don't cut mid-sentence.
        if let Some(pos) = chat.rfind('\n') {
            chat.truncate(pos);
        }
    }

    let timeout = match &opts.backend {
        ApiBackend::ClaudeCodeCli => std::time::Duration::from_secs(120),
        _ => std::time::Duration::from_secs(30),
    };
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let user = user_prompt(&chat, files, repo_type);

    let raw = call_chat(&client, &opts.backend, &opts.model, SYSTEM_PROMPT, &user, opts.temperature, opts.top_p, opts.max_completion_tokens).await?;

    let mut items = parse_context_items(&raw);

    // Repair pass: if model returned *something* but not parseable into our schema.
    if items.is_empty() && raw.trim() != "" && raw.trim() != "[]" {
        let repair_system = "You are a strict JSON converter. Convert the input into the required JSON array only.";
        let repair_user = format!(
            r#"
You MUST output a JSON ARRAY (list) of 0-5 objects.
Each object keys: type, title, summary, tags, file (optional).
No other keys. No markdown fences. Strict JSON only.

If the input is not about engineering insights, output [].

CHAT (sanitized):
{chat}

MODEL OUTPUT TO CONVERT:
{raw}
"#,
            chat = chat,
            raw = raw
        );

        let repaired_raw = call_chat(&client, &opts.backend, &opts.repair_model, repair_system, &repair_user, 0.0, 0.95, opts.max_completion_tokens)
            .await
            .unwrap_or_default();

        if !repaired_raw.trim().is_empty() {
            items = parse_context_items(&repaired_raw);
        }
    }

    // Deduplicate by summary (case-insensitive), mirroring Python behavior.
    let mut seen = std::collections::HashSet::<String>::new();
    items.retain(|it| {
        let key = it.summary.to_lowercase();
        if key.trim().is_empty() {
            false
        } else if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });

    if items.len() > 10 {
        items.truncate(10);
    }

    let debug = if opts.debug_raw_output { Some(raw) } else { None };
    Ok((items, debug))
}

