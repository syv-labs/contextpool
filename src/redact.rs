//! Secret redaction for chat transcripts before they are sent to an LLM.
//!
//! Design principle: **prefer false-positives over leaking secrets.**
//! A redacted word never causes harm; a leaked key can.

use regex::Regex;
use std::sync::LazyLock;

const REDACTED: &str = "[REDACTED]";

/// Redact secrets from a transcript string.
/// Applies all redaction passes in order: structured patterns first, then inline scans.
pub fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for line in text.lines() {
        let l = line.trim_end();
        // Try structured line-level redactions first (env assignments)
        if let Some(redacted_line) = try_redact_env_assignment(l) {
            out.push_str(&redacted_line);
            out.push('\n');
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }

    // Multi-line: private key blocks
    out = redact_private_key_blocks(&out);

    // Inline pattern replacements (applied to the full text)
    out = redact_known_prefixes(&out);
    out = redact_bearer_tokens(&out);
    out = redact_connection_strings(&out);
    out = redact_jwt_tokens(&out);
    out = redact_high_entropy_assignments(&out);

    out
}

// ── Line-level: environment variable assignments ─────────────────────────────

/// Sensitive key name suffixes and substrings.
const SENSITIVE_PATTERNS: &[&str] = &[
    "_KEY", "_TOKEN", "_SECRET", "_PASSWORD", "_PASSWD", "_CREDENTIALS",
    "API_KEY", "APIKEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD",
    "AUTH", "PRIVATE", "CREDENTIALS",
];

fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_uppercase();
    SENSITIVE_PATTERNS.iter().any(|pat| upper.contains(pat))
}

fn try_redact_env_assignment(line: &str) -> Option<String> {
    let trimmed = line.trim_start();

    // `export FOO=...`
    if let Some(rest) = trimmed.strip_prefix("export ") {
        if let Some((k, _v)) = rest.split_once('=') {
            let key = k.trim();
            if is_sensitive_key(key) {
                return Some(format!("export {}={}", key, REDACTED));
            }
        }
        return None;
    }

    // Bare `FOO=...` (no spaces in key, to avoid matching arbitrary text)
    if let Some((k, _v)) = trimmed.split_once('=') {
        let key = k.trim();
        if !key.is_empty() && !key.contains(' ') && is_sensitive_key(key) {
            return Some(format!("{}={}", key, REDACTED));
        }
    }

    None
}

// ── Known secret prefixes ────────────────────────────────────────────────────
// These are well-known prefixes used by cloud providers and SaaS APIs.
// We match the prefix + a run of token-like characters and replace the whole thing.

static KNOWN_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?:",
        // AWS access key IDs (always start with AKIA/ASIA)
        r"(?:AKIA|ASIA)[A-Z0-9]{16}",
        r"|",
        // GitHub tokens (classic PAT, fine-grained, OAuth, app)
        r"(?:ghp_|gho_|ghs_|ghr_|github_pat_)[A-Za-z0-9_]{20,}",
        r"|",
        // Anthropic API keys
        r"sk-ant-[A-Za-z0-9\-_]{20,}",
        r"|",
        // OpenAI API keys
        r"sk-[A-Za-z0-9]{20,}",
        r"|",
        // Stripe keys (secret, publishable, restricted)
        r"(?:sk_live_|pk_live_|rk_live_|sk_test_|pk_test_|rk_test_)[A-Za-z0-9]{10,}",
        r"|",
        // Slack tokens
        r"xox[bpaosr]-[A-Za-z0-9\-]{10,}",
        r"|",
        // Twilio
        r"SK[a-f0-9]{32}",
        r"|",
        // SendGrid
        r"SG\.[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,}",
        r"|",
        // npm tokens
        r"npm_[A-Za-z0-9]{20,}",
        r"|",
        // PyPI tokens
        r"pypi-[A-Za-z0-9\-_]{20,}",
        r"|",
        // Vercel tokens
        r"(?:vercel_|vc_prod_)[A-Za-z0-9]{20,}",
        r"|",
        // Supabase
        r"sbp_[A-Za-z0-9]{20,}",
        r"|",
        // Heroku
        r"(?:heroku_|HRKU-)[A-Za-z0-9\-]{20,}",
        r"|",
        // Postman
        r"PMAK-[A-Za-z0-9\-]{20,}",
        r"|",
        // HuggingFace
        r"hf_[A-Za-z0-9]{20,}",
        r"|",
        // Databricks
        r"dapi[a-f0-9]{32}",
        r"|",
        // DigitalOcean
        r"dop_v1_[a-f0-9]{64}",
        r"|",
        // Replicate
        r"r8_[A-Za-z0-9]{20,}",
        r")",
    ))
    .expect("known prefix regex")
});

fn redact_known_prefixes(text: &str) -> String {
    KNOWN_PREFIX_RE.replace_all(text, REDACTED).into_owned()
}

// ── Bearer tokens ────────────────────────────────────────────────────────────

static BEARER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:Bearer|Authorization[:\s]+Bearer)\s+)[A-Za-z0-9\-_\.]{20,}")
        .expect("bearer regex")
});

fn redact_bearer_tokens(text: &str) -> String {
    BEARER_RE
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}", &caps[1], REDACTED)
        })
        .into_owned()
}

// ── Connection strings ───────────────────────────────────────────────────────
// postgres://user:pass@host, mongodb+srv://user:pass@host, redis://..., mysql://...

static CONN_STRING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)((?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp|mssql)://[^:\s]+:)[^\s@]+(@)"
    )
    .expect("connection string regex")
});

fn redact_connection_strings(text: &str) -> String {
    CONN_STRING_RE
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}{}", &caps[1], REDACTED, &caps[2])
        })
        .into_owned()
}

// ── JWT tokens ───────────────────────────────────────────────────────────────
// eyJ... tokens (three base64url-encoded parts separated by dots)

static JWT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_\-]{10,}")
        .expect("jwt regex")
});

fn redact_jwt_tokens(text: &str) -> String {
    JWT_RE.replace_all(text, REDACTED).into_owned()
}

// ── Private key blocks ───────────────────────────────────────────────────────

static PRIVATE_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----")
        .expect("private key regex")
});

fn redact_private_key_blocks(text: &str) -> String {
    PRIVATE_KEY_RE.replace_all(text, REDACTED).into_owned()
}

// ── High-entropy values in key-value assignments ─────────────────────────────
// Catches: api_key: "abc123...", "token": "abc123...", password = "abc123..."
// Only fires when the key name looks secret-related.

/// Matches key-value assignments where the key is secret-like and the value is
/// either double-quoted, single-quoted, or a bare token of 12+ chars.
static KV_DOUBLE_QUOTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?(?:api[_-]?key|token|secret|password|passwd|credentials|auth[_-]?token|access[_-]?key|private[_-]?key|client[_-]?secret|signing[_-]?key|encryption[_-]?key)["']?\s*[:=]\s*)"([^"]{8,})""#
    )
    .expect("kv double-quoted regex")
});

static KV_SINGLE_QUOTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?(?:api[_-]?key|token|secret|password|passwd|credentials|auth[_-]?token|access[_-]?key|private[_-]?key|client[_-]?secret|signing[_-]?key|encryption[_-]?key)["']?\s*[:=]\s*)'([^']{8,})'"#
    )
    .expect("kv single-quoted regex")
});

static KV_UNQUOTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?(?:api[_-]?key|token|secret|password|passwd|credentials|auth[_-]?token|access[_-]?key|private[_-]?key|client[_-]?secret|signing[_-]?key|encryption[_-]?key)["']?\s*[:=]\s*)[A-Za-z0-9\-_\.+/=]{12,}"#
    )
    .expect("kv unquoted regex")
});

fn redact_high_entropy_assignments(text: &str) -> String {
    let mut out = text.to_string();
    // Double-quoted values
    out = KV_DOUBLE_QUOTED_RE
        .replace_all(&out, |caps: &regex::Captures| {
            format!("{}\"{}\"", &caps[1], REDACTED)
        })
        .into_owned();
    // Single-quoted values
    out = KV_SINGLE_QUOTED_RE
        .replace_all(&out, |caps: &regex::Captures| {
            format!("{}'{}'", &caps[1], REDACTED)
        })
        .into_owned();
    // Unquoted bare values
    out = KV_UNQUOTED_RE
        .replace_all(&out, |caps: &regex::Captures| {
            format!("{}{}", &caps[1], REDACTED)
        })
        .into_owned();
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_export() {
        let input = "export ANTHROPIC_API_KEY=sk-ant-abc123def456";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("sk-ant-abc123def456"));
    }

    #[test]
    fn test_bare_env() {
        let input = "OPENAI_API_KEY=sk-1234567890abcdef";
        let out = redact_secrets(input);
        assert!(out.contains("OPENAI_API_KEY=[REDACTED]"));
        assert!(!out.contains("sk-1234567890abcdef"));
    }

    #[test]
    fn test_aws_access_key() {
        let input = "My AWS key is AKIAIOSFODNN7EXAMPLE and it works.";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_github_pat() {
        let input = "ghp_ABCDEFghijklmnopqrstuvwxyz1234567890";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_anthropic_key() {
        let input = "Using sk-ant-api03-abcdefghijklmnopqrstuvwxyz for auth.";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("sk-ant-api03"));
    }

    #[test]
    fn test_openai_key() {
        let input = "sk-abcdefghijklmnopqrstuvwxyz123456";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_stripe_key() {
        let input = "sk_live_abcdefghijklmnopqrstuvwxyz";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_slack_token() {
        let input = "xoxb-123456789012-abcdefghijklmn";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc.def";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("eyJhbG"));
    }

    #[test]
    fn test_connection_string() {
        let input = "DATABASE_URL=postgres://admin:supersecretpass@db.example.com:5432/mydb";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("supersecretpass"));
    }

    #[test]
    fn test_jwt() {
        let input = "token: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("eyJhbG"));
    }

    #[test]
    fn test_private_key_block() {
        let input = "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----\nDone.";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("MIIEpAIBAAKCAQEA"));
    }

    #[test]
    fn test_json_api_key_field() {
        let input = r#""api_key": "abcdef123456789012345""#;
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("abcdef123456789012345"));
    }

    #[test]
    fn test_yaml_token_field() {
        let input = "token: abcdefghijklmnopqrstuvwxyz";
        let out = redact_secrets(input);
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn test_normal_text_untouched() {
        let input = "User: I fixed the bug in the authentication module.\nAssistant: Great, the token validation logic looks correct now.";
        let out = redact_secrets(input);
        // "token" as normal English should not trigger false positives on the whole line
        assert!(out.contains("I fixed the bug"));
        assert!(out.contains("validation logic looks correct"));
    }

    #[test]
    fn test_npm_token() {
        let input = "npm_abcdefghijklmnopqrstuvwxyz";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_huggingface_token() {
        let input = "hf_abcdefghijklmnopqrstuvwxyz";
        let out = redact_secrets(input);
        assert_eq!(out.trim(), "[REDACTED]");
    }

    #[test]
    fn test_multiple_secrets_one_line() {
        let input = "Keys: ghp_aaaabbbbccccddddeeeeffffggg and sk-ant-xxxxyyyyzzzz1234567890abcdef";
        let out = redact_secrets(input);
        assert!(!out.contains("ghp_aaaa"));
        assert!(!out.contains("sk-ant-xxxx"));
        assert_eq!(out.matches("[REDACTED]").count(), 2);
    }
}
