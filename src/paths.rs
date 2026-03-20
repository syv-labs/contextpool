use std::path::PathBuf;

pub fn default_out_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("ContextPool")
}

pub fn default_cursor_dir() -> Option<PathBuf> {
    // Cursor tends to use `~/.cursor` on macOS and often on other platforms too,
    // but prefer platform-specific app-data locations when present.
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".cursor"));
    }

    if let Some(local) = dirs::data_local_dir() {
        // Common on Windows: %LOCALAPPDATA%\Cursor (or variants)
        candidates.push(local.join("Cursor"));
        candidates.push(local.join("cursor"));
    }

    if let Some(data) = dirs::data_dir() {
        // Sometimes used on Linux: ~/.local/share/Cursor or ~/.config/Cursor-like layouts
        candidates.push(data.join("Cursor"));
        candidates.push(data.join("cursor"));
    }

    // Linux config dir (and sometimes macOS): ~/.config/Cursor
    if let Some(cfg) = dirs::config_dir() {
        candidates.push(cfg.join("Cursor"));
        candidates.push(cfg.join("cursor"));
    }

    // Heuristic: Cursor stores transcripts under `<cursor_dir>/projects/*/agent-transcripts`.
    for c in candidates {
        if c.join("projects").exists() {
            return Some(c);
        }
        if c.join("agent-transcripts").exists() {
            return Some(c);
        }
    }

    None
}

pub fn default_claude_code_dir() -> Option<PathBuf> {
    // Claude Code stores conversations under ~/.claude on all platforms.
    let home = dirs::home_dir()?;
    let candidate = home.join(".claude");
    if candidate.join("projects").exists() {
        return Some(candidate);
    }
    // Return the path even if projects/ doesn't exist yet so callers can report a useful error.
    if candidate.exists() {
        return Some(candidate);
    }
    None
}

pub fn default_workspace_storage_dir(product: &str) -> Option<PathBuf> {
    let product = if product.trim().is_empty() {
        "Cursor"
    } else {
        product.trim()
    };

    let home = dirs::home_dir()?;

    // VS Code-style layout:
    // - macOS:   ~/Library/Application Support/<Product>/User/workspaceStorage
    // - Windows: %APPDATA%\<Product>\User\workspaceStorage
    // - Linux:   ~/.config/<Product>/User/workspaceStorage  (common)
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join(product)
                .join("User")
                .join("workspaceStorage"),
        );
    }

    if let Some(appdata) = dirs::data_dir() {
        // On Windows, dirs::data_dir() typically points at %APPDATA%.
        candidates.push(
            appdata
                .join(product)
                .join("User")
                .join("workspaceStorage"),
        );
    }

    if let Some(cfg) = dirs::config_dir() {
        candidates.push(cfg.join(product).join("User").join("workspaceStorage"));
    }

    // Try some well-known product name variants for popular forks.
    if product.eq_ignore_ascii_case("windsurf") {
        if let Some(cfg) = dirs::config_dir() {
            candidates.push(cfg.join("Windsurf").join("User").join("workspaceStorage"));
            candidates.push(cfg.join("Codeium").join("Windsurf").join("User").join("workspaceStorage"));
        }
        if let Some(appdata) = dirs::data_dir() {
            candidates.push(appdata.join("Windsurf").join("User").join("workspaceStorage"));
            candidates.push(appdata.join("Codeium").join("Windsurf").join("User").join("workspaceStorage"));
        }
    }

    if product.eq_ignore_ascii_case("vscode") || product.eq_ignore_ascii_case("code") {
        if let Some(cfg) = dirs::config_dir() {
            candidates.push(cfg.join("Code").join("User").join("workspaceStorage"));
            candidates.push(cfg.join("Visual Studio Code").join("User").join("workspaceStorage"));
        }
        if let Some(appdata) = dirs::data_dir() {
            candidates.push(appdata.join("Code").join("User").join("workspaceStorage"));
            candidates.push(appdata.join("Visual Studio Code").join("User").join("workspaceStorage"));
        }
    }

    if product.eq_ignore_ascii_case("cursor") {
        if let Some(cfg) = dirs::config_dir() {
            candidates.push(cfg.join("Cursor").join("User").join("workspaceStorage"));
        }
        if let Some(appdata) = dirs::data_dir() {
            candidates.push(appdata.join("Cursor").join("User").join("workspaceStorage"));
        }
    }

    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }

    None
}

