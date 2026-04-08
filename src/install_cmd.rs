use crate::cli::InstallArgs;
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Write `content` to `path` atomically (temp file + rename).
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

/// Read a JSON file, returning an empty object `{}` if the file does not exist.
fn read_json_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Reading {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Parsing JSON in {}", path.display()))
}

// ── Claude Code ───────────────────────────────────────────────────────────────

/// Default path for Claude Code's global config file.
fn default_claude_json() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude.json")
}

fn configure_claude_code(claude_json: &Path, binary_path: &str, force: bool) -> Result<bool> {
    let mut root = read_json_or_empty(claude_json)?;

    // Ensure mcpServers object exists.
    if !root.get("mcpServers").map(|v| v.is_object()).unwrap_or(false) {
        root["mcpServers"] = serde_json::json!({});
    }

    let servers = root["mcpServers"].as_object_mut().unwrap();

    if servers.contains_key("contextpool") && !force {
        return Ok(false); // already configured, skip
    }

    servers.insert(
        "contextpool".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": binary_path,
            "args": ["mcp"]
        }),
    );

    let pretty = serde_json::to_string_pretty(&root)?;
    atomic_write(claude_json, &pretty)?;
    Ok(true)
}

// ── Cursor ────────────────────────────────────────────────────────────────────

/// Default path for Cursor's global MCP config.
fn default_cursor_mcp_json() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cursor")
        .join("mcp.json")
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
        serde_json::json!({
            "command": binary_path,
            "args": ["mcp"]
        }),
    );

    let pretty = serde_json::to_string_pretty(&root)?;
    atomic_write(cursor_mcp, &pretty)?;
    Ok(true)
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn cmd_install(args: InstallArgs) -> Result<()> {
    // Determine the binary path to register.
    // Prefer --binary-path if given; fall back to the running executable.
    let binary_path = if let Some(p) = args.binary_path {
        p.to_string_lossy().to_string()
    } else {
        std::env::current_exe()
            .context("Cannot determine current executable path. Use --binary-path to set it explicitly.")?
            .to_string_lossy()
            .to_string()
    };

    let claude_json = args
        .claude_json
        .unwrap_or_else(default_claude_json);

    let cursor_mcp = args
        .cursor_mcp
        .unwrap_or_else(default_cursor_mcp_json);

    let force = args.force;

    let mut configured_any = false;

    // ── Claude Code ──
    if !args.skip_claude {
        match configure_claude_code(&claude_json, &binary_path, force) {
            Ok(true) => {
                println!("✓ Claude Code — added contextpool to {}", claude_json.display());
                configured_any = true;
            }
            Ok(false) => {
                println!(
                    "  Claude Code — contextpool already in {} (use --force to overwrite)",
                    claude_json.display()
                );
            }
            Err(e) => {
                eprintln!("✗ Claude Code — failed to update {}: {e}", claude_json.display());
            }
        }
    }

    // ── Cursor ──
    if !args.skip_cursor {
        match configure_cursor(&cursor_mcp, &binary_path, force) {
            Ok(true) => {
                println!("✓ Cursor — added contextpool to {}", cursor_mcp.display());
                configured_any = true;
            }
            Ok(false) => {
                println!(
                    "  Cursor — contextpool already in {} (use --force to overwrite)",
                    cursor_mcp.display()
                );
            }
            Err(e) => {
                eprintln!("✗ Cursor — failed to update {}: {e}", cursor_mcp.display());
            }
        }
    }

    if configured_any {
        println!();
        println!("Restart your IDE(s) to activate the contextpool MCP server.");
    }

    Ok(())
}
