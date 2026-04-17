use crate::{
    cli::InstallArgs,
    credentials::{
        load_anthropic_api_key_stored, load_api_backend, load_backend_preference,
        load_openai_api_key_stored, save_anthropic_api_key, save_backend_preference,
        save_openai_api_key, ApiBackend, BackendPreference,
    },
};
use anyhow::{Context, Result};
use std::{
    fs,
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};
use toml_edit::{value, Array, DocumentMut, Item, Table};

// ── helpers ───────────────────────────────────────────────────────────────────

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Creating directory {}", parent.display()))?;
    }
    fs::write(&tmp, content).with_context(|| format!("Writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("Renaming to {}", path.display()))?;
    Ok(())
}

fn read_json_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Reading {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Parsing JSON in {}", path.display()))
}

fn claude_cli_in_path() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join("claude").exists())
}

// ── MCP config ────────────────────────────────────────────────────────────────

fn default_claude_json() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude.json")
}

fn default_cursor_mcp_json() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cursor")
        .join("mcp.json")
}

fn default_codex_config_toml() -> PathBuf {
    // Respect $CODEX_HOME if set (Codex CLI allows overriding the data directory).
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codex_home).join("config.toml");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("config.toml")
}

fn default_kiro_json() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".kiro")
        .join("settings")
        .join("mcp.json")
}

fn configure_codex(config_toml: &Path, binary_path: &str, force: bool) -> Result<bool> {
    // Read or create the TOML document.
    let raw = if config_toml.exists() {
        fs::read_to_string(config_toml)
            .with_context(|| format!("Reading {}", config_toml.display()))?
    } else {
        String::new()
    };

    let mut doc = raw.parse::<DocumentMut>()
        .with_context(|| format!("Parsing TOML in {}", config_toml.display()))?;

    // Ensure [mcp_servers] table exists.
    if !doc.contains_key("mcp_servers") {
        doc["mcp_servers"] = Item::Table(Table::new());
    }
    let mcp_servers = doc["mcp_servers"].as_table_mut()
        .context("[mcp_servers] is not a table")?;

    if mcp_servers.contains_key("contextpool") && !force {
        return Ok(false);
    }

    // Build the [mcp_servers.contextpool] entry.
    let mut entry = Table::new();
    entry["command"] = value(binary_path);
    let mut args = Array::new();
    args.push("mcp");
    entry["args"] = value(args);

    mcp_servers["contextpool"] = Item::Table(entry);

    atomic_write(config_toml, &doc.to_string())?;
    Ok(true)
}

fn configure_claude_code(claude_json: &Path, binary_path: &str, force: bool) -> Result<bool> {
    let mut root = read_json_or_empty(claude_json)?;
    if !root.get("mcpServers").map(|v| v.is_object()).unwrap_or(false) {
        root["mcpServers"] = serde_json::json!({});
    }
    let servers = root["mcpServers"].as_object_mut().unwrap();
    if servers.contains_key("contextpool") && !force {
        return Ok(false);
    }
    servers.insert(
        "contextpool".to_string(),
        serde_json::json!({ "type": "stdio", "command": binary_path, "args": ["mcp"] }),
    );
    atomic_write(claude_json, &serde_json::to_string_pretty(&root)?)?;
    Ok(true)
}

fn configure_cursor(cursor_mcp: &Path, binary_path: &str, force: bool) -> Result<bool> {
    let mut root = read_json_or_empty(cursor_mcp)?;
    if !root.get("mcpServers").map(|v| v.is_object()).unwrap_or(false) {
        root["mcpServers"] = serde_json::json!({});
    }
    let servers = root["mcpServers"].as_object_mut().unwrap();
    if servers.contains_key("contextpool") && !force {
        return Ok(false);
    }
    servers.insert(
        "contextpool".to_string(),
        serde_json::json!({ "command": binary_path, "args": ["mcp"] }),
    );
    atomic_write(cursor_mcp, &serde_json::to_string_pretty(&root)?)?;
    Ok(true)
}

fn configure_kiro(kiro_json: &Path, binary_path: &str, force: bool) -> Result<bool> {
    let mut root = read_json_or_empty(kiro_json)?;
    
    // Ensure the mcpServers object exists
    if !root.get("mcpServers").map(|v| v.is_object()).unwrap_or(false) {
        root["mcpServers"] = serde_json::json!({});
    }
    
    let servers = root["mcpServers"].as_object_mut().unwrap();
    
    // Check if contextpool is already configured
    if servers.contains_key("contextpool") && !force {
        return Ok(false);
    }
    
    // Insert the contextpool MCP server configuration
    servers.insert(
        "contextpool".to_string(),
        serde_json::json!({ "command": binary_path, "args": ["mcp"] }),
    );
    
    atomic_write(kiro_json, &serde_json::to_string_pretty(&root)?)?;
    Ok(true)
}

// ── Backend setup wizard ──────────────────────────────────────────────────────

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "****".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

fn current_backend_summary() -> Option<String> {
    match load_api_backend()? {
        ApiBackend::ClaudeCodeCli    => Some("Claude Code CLI".to_string()),
        ApiBackend::Anthropic(k)     => Some(format!("Anthropic API ({})", mask_key(&k))),
        ApiBackend::OpenAI(k)        => Some(format!("OpenAI API ({})", mask_key(&k))),
        ApiBackend::Nvidia(k)        => Some(format!("NVIDIA NIM ({})", mask_key(&k))),
    }
}

fn prompt_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn prompt_key(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let key = rpassword::read_password().context("Failed to read key")?;
    Ok(key.trim().to_string())
}

fn run_setup_wizard() -> Result<()> {
    let claude_available = claude_cli_in_path();

    println!();
    println!("── LLM backend setup ───────────────────────────────────────────");
    println!();
    println!("Which backend should ContextPool use for summarization?");
    println!();

    if claude_available {
        println!("  1) Claude Code  — free, uses your Claude Code subscription");
        println!("                    (slower, ~1 subprocess at a time)");
    } else {
        println!("  1) Claude Code  — (not detected in PATH, will likely fail)");
    }
    println!("  2) Anthropic API — direct API, billed per token, fastest");
    println!("                    (parallelizes well, works headless)");
    println!("  3) OpenAI API");
    println!("  4) NVIDIA NIM");
    println!("  5) Skip — I'll configure this later");
    println!();

    let default = if claude_available { "1" } else { "2" };
    let choice = prompt_line(&format!("Choice [{default}]: "))?;
    let choice = if choice.is_empty() { default.to_string() } else { choice };

    match choice.as_str() {
        "1" => {
            save_backend_preference(&BackendPreference::ClaudeCode)?;
            if claude_available {
                println!("✓ Backend set to Claude Code CLI.");
            } else {
                println!("  Warning: `claude` not found in PATH. Install Claude Code first.");
                println!("  Backend preference saved; will activate once `claude` is available.");
            }
        }
        "2" => {
            // Check if a key is already stored
            let existing = std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .or_else(load_anthropic_api_key_stored);

            let key = if let Some(k) = existing {
                println!("  Found existing Anthropic API key ({}).", mask_key(&k));
                let reuse = prompt_line("  Use this key? [Y/n]: ")?;
                if reuse.is_empty() || reuse.eq_ignore_ascii_case("y") {
                    k
                } else {
                    let k = prompt_key("  Paste new Anthropic API key (sk-ant-...): ")?;
                    if k.is_empty() { anyhow::bail!("Empty key — aborting backend setup."); }
                    k
                }
            } else {
                let k = prompt_key("  Paste Anthropic API key (sk-ant-...): ")?;
                if k.is_empty() { anyhow::bail!("Empty key — aborting backend setup."); }
                k
            };

            save_anthropic_api_key(&key)?;
            save_backend_preference(&BackendPreference::Anthropic)?;
            println!("✓ Anthropic API key saved. Backend set to Anthropic API.");
        }
        "3" => {
            let existing = std::env::var("OPENAI_API_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .or_else(load_openai_api_key_stored);

            let key = if let Some(k) = existing {
                println!("  Found existing OpenAI API key ({}).", mask_key(&k));
                let reuse = prompt_line("  Use this key? [Y/n]: ")?;
                if reuse.is_empty() || reuse.eq_ignore_ascii_case("y") {
                    k
                } else {
                    let k = prompt_key("  Paste new OpenAI API key (sk-...): ")?;
                    if k.is_empty() { anyhow::bail!("Empty key — aborting backend setup."); }
                    k
                }
            } else {
                let k = prompt_key("  Paste OpenAI API key (sk-...): ")?;
                if k.is_empty() { anyhow::bail!("Empty key — aborting backend setup."); }
                k
            };

            save_openai_api_key(&key)?;
            save_backend_preference(&BackendPreference::OpenAI)?;
            println!("✓ OpenAI API key saved. Backend set to OpenAI API.");
        }
        "4" => {
            // NVIDIA uses the existing interactive flow from credentials.rs
            match crate::credentials::ensure_nvidia_api_key_interactive() {
                Ok(_) => {
                    save_backend_preference(&BackendPreference::Nvidia)?;
                    println!("✓ Backend set to NVIDIA NIM.");
                }
                Err(e) => eprintln!("  Warning: {e}"),
            }
        }
        "5" | _ => {
            println!("  Skipped. Run `cxp install --setup` to configure later.");
            println!("  Or set ANTHROPIC_API_KEY / OPENAI_API_KEY in your environment.");
        }
    }

    println!();
    Ok(())
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn cmd_install(args: InstallArgs) -> Result<()> {
    // `--setup` alone: skip MCP registration and jump straight to wizard.
    if args.setup {
        return run_setup_wizard();
    }

    let binary_path = if let Some(p) = args.binary_path {
        p.to_string_lossy().to_string()
    } else {
        std::env::current_exe()
            .context("Cannot determine current executable path. Use --binary-path to set it explicitly.")?
            .to_string_lossy()
            .to_string()
    };

    let claude_json      = args.claude_json.unwrap_or_else(default_claude_json);
    let cursor_mcp       = args.cursor_mcp.unwrap_or_else(default_cursor_mcp_json);
    let codex_config     = default_codex_config_toml();
    let force            = args.force;

    let mut configured_any = false;

    // ── MCP server registration ──
    if !args.skip_claude {
        match configure_claude_code(&claude_json, &binary_path, force) {
            Ok(true)  => { println!("✓ Claude Code — added contextpool to {}", claude_json.display()); configured_any = true; }
            Ok(false) => { println!("  Claude Code — contextpool already in {} (use --force to overwrite)", claude_json.display()); }
            Err(e)    => { eprintln!("✗ Claude Code — failed to update {}: {e}", claude_json.display()); }
        }
    }

    if !args.skip_cursor {
        match configure_cursor(&cursor_mcp, &binary_path, force) {
            Ok(true)  => { println!("✓ Cursor — added contextpool to {}", cursor_mcp.display()); configured_any = true; }
            Ok(false) => { println!("  Cursor — contextpool already in {} (use --force to overwrite)", cursor_mcp.display()); }
            Err(e)    => { eprintln!("✗ Cursor — failed to update {}: {e}", cursor_mcp.display()); }
        }
    }

    if !args.skip_codex {
        match configure_codex(&codex_config, &binary_path, force) {
            Ok(true)  => { println!("✓ Codex — added contextpool to {}", codex_config.display()); configured_any = true; }
            Ok(false) => { println!("  Codex — contextpool already in {} (use --force to overwrite)", codex_config.display()); }
            Err(e)    => { eprintln!("✗ Codex — failed to update {}: {e}", codex_config.display()); }
        }
    }

    if !args.skip_kiro {
          let kiro_mcp = args.kiro_mcp.unwrap_or_else(default_kiro_json);
          match configure_kiro(&kiro_mcp, &binary_path, force) {
              Ok(true)  => { println!("✓ Kiro — added contextpool to {}", kiro_mcp.display()); configured_any = true; }
              Ok(false) => { println!("  Kiro — contextpool already in {} (use --force to overwrite)", kiro_mcp.display()); }
              Err(e)    => { eprintln!("✗ Kiro — failed to update {}: {e}", kiro_mcp.display()); }
          }
    }

    if configured_any {
        println!();
        println!("Restart your IDE(s) to activate the contextpool MCP server.");
    }

    // ── Backend setup wizard ──
    if args.skip_setup {
        return Ok(());
    }

    // If already configured and not forcing re-setup, show status and skip.
    if !args.force {
        if let Some(summary) = current_backend_summary() {
            if load_backend_preference().is_some() {
                println!();
                println!("  LLM backend: {summary}");
                println!("  Run `cxp install --setup` to reconfigure.");
                return Ok(());
            }
        }
    }

    // Only run wizard in an interactive terminal.
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        if load_api_backend().is_none() {
            eprintln!();
            eprintln!("  No LLM backend configured. Run `cxp install --setup` interactively,");
            eprintln!("  or set ANTHROPIC_API_KEY / OPENAI_API_KEY in your environment.");
        }
        return Ok(());
    }

    run_setup_wizard()
}
