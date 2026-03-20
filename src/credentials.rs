use anyhow::{Context, Result};
use std::{
    io::{self, IsTerminal, Write},
    sync::OnceLock,
};

const KEYRING_SERVICE: &str = "contextpool";
const KEYRING_USER: &str = "default";
const ENV_KEY: &str = "NVIDIA_API_KEY";

static CACHED_KEY: OnceLock<String> = OnceLock::new();

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

    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if let Ok(v) = entry.get_password() {
            let t = v.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }

    None
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

    Ok(key)
}

