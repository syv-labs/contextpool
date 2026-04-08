use anyhow::{Context, Result};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    sync::OnceLock,
};

const KEYRING_SERVICE: &str = "contextpool";
const KEYRING_USER: &str = "default";
const ENV_KEY: &str = "NVIDIA_API_KEY";
const ANTHROPIC_ENV_KEY: &str = "ANTHROPIC_API_KEY";
const API_KEY_FILE_NAME: &str = "nvidia_api_key";

/// Which LLM backend to use for summarization.
#[derive(Clone, Debug)]
pub enum ApiBackend {
    /// Use `claude -p` subprocess — inherits Claude Code / Cursor auth, no API key needed.
    ClaudeCodeCli,
    /// Anthropic Claude (uses the Messages API).
    Anthropic(String),
    /// OpenAI chat completions API.
    OpenAI(String),
    /// NVIDIA NIM / OpenAI-compatible endpoint.
    Nvidia(String),
}

/// Priority: claude CLI → ANTHROPIC_API_KEY → OPENAI_API_KEY → NVIDIA key chain.
/// Returns `None` only if no backend is available at all.
pub fn load_api_backend() -> Option<ApiBackend> {
    if claude_cli_in_path() {
        return Some(ApiBackend::ClaudeCodeCli);
    }
    if let Ok(v) = std::env::var(ANTHROPIC_ENV_KEY) {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(ApiBackend::Anthropic(t));
        }
    }
    if let Ok(v) = std::env::var("OPENAI_API_KEY") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(ApiBackend::OpenAI(t));
        }
    }
    load_nvidia_api_key().map(ApiBackend::Nvidia)
}

fn claude_cli_in_path() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join("claude").exists())
}

static CACHED_KEY: OnceLock<String> = OnceLock::new();

fn api_key_file_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("ContextPool")
        .join(API_KEY_FILE_NAME)
}

fn load_api_key_from_file() -> Option<String> {
    let path = api_key_file_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return None;
    };
    let key = raw.trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some(key)
}

fn save_api_key_to_file(key: &str) -> Result<()> {
    let path = api_key_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    fs::write(&path, key).with_context(|| format!("Writing {}", path.display()))?;

    // Best-effort permissions hardening (0600).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

fn delete_api_key_file() -> Result<()> {
    let path = api_key_file_path();
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed deleting {}: {e}", path.display())),
    }
}

pub fn load_nvidia_api_key() -> Option<String> {
    if let Some(k) = CACHED_KEY.get() {
        return Some(k.clone());
    }

    if let Ok(v) = std::env::var(ENV_KEY) {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }

    // Check the file-based cache before the keychain. The file is written on
    // first interactive entry or successful keychain read. This path is fast,
    // non-blocking, and works in headless/MCP subprocess contexts where
    // the keychain may display a UI permission dialog.
    if let Some(k) = load_api_key_from_file() {
        return Some(k);
    }

    // Keychain lookup: on macOS this is a blocking syscall that can pop a UI
    // dialog in headless contexts (MCP subprocesses), potentially hanging for
    // 30+ seconds. Run it in a background thread with a timeout so the Tokio
    // async runtime is never stalled.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            if let Ok(v) = entry.get_password() {
                let t = v.trim().to_string();
                if !t.is_empty() {
                    let _ = tx.send(t);
                }
            }
        }
    });
    rx.recv_timeout(std::time::Duration::from_secs(3)).ok()
}

pub fn ensure_nvidia_api_key_interactive() -> Result<String> {
    if let Some(k) = load_nvidia_api_key() {
        return Ok(k);
    }

    // If we're not in an interactive terminal, don't hang waiting for input.
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        anyhow::bail!(
            "Missing NVIDIA API key. Set {} env var or run interactively to enter it.",
            ENV_KEY
        );
    }

    eprint!("Enter NVIDIA API key: ");
    io::stderr().flush().ok();
    let key = rpassword::read_password().context("Failed to read API key")?;
    let key = key.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("Empty API key entered.");
    }

    // Cache in memory for the rest of this process — no more re-prompting per file.
    let _ = CACHED_KEY.set(key.clone());

    // Best-effort persistence to keychain.
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if entry.set_password(&key).is_ok() {
            eprintln!("Saved API key to system keychain for future runs.");
        }
    }

    // Also persist to a local 0600 cache file so new processes can still find it
    // even if keychain access fails for some reason.
    if save_api_key_to_file(&key).is_ok() {
        eprintln!("Saved API key for future runs.");
    }

    Ok(key)
}

/// Deletes the stored NVIDIA API key from the system keychain (best-effort).
/// This forces `ensure_nvidia_api_key_interactive()` to re-prompt on the next call.
pub fn reset_nvidia_api_key() -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER);
    let Ok(entry) = entry else {
        // If we can't even initialize the keychain entry, still don't crash the CLI.
        eprintln!("Warning: could not initialize keychain entry; continuing without reset.");
        return Ok(());
    };

    match entry.delete_credential() {
        Ok(()) => eprintln!("Cleared NVIDIA API key from system keychain."),
        Err(keyring::Error::NoEntry) => {
            eprintln!("No saved NVIDIA API key found in system keychain.");
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to clear NVIDIA API key from system keychain: {e}"
            ))
        }
    }

    // Also clear the local file fallback.
    let _ = delete_api_key_file();

    Ok(())
}

