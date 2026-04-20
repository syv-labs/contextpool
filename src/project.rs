use std::path::{Path, PathBuf};

/// Derive a stable, team-consistent project id from the current working directory.
///
/// Priority:
///   1. `git remote get-url origin` — canonical across all team members cloning the same repo
///      e.g. `https://github.com/org/my-api.git` → `org-my-api`
///   2. Git repo with no remote — use repo root dir name only (stable across machines)
///      e.g. `/Users/alice/dev/my-api` (root) → `alice-my-api`
///   3. Not a git repo — use `git config --global user.name` + folder name
///      e.g. username=alice, dir=my-scripts → `alice-my-scripts`
///   4. Absolute last resort — directory name (old behaviour, single-user only)
pub fn project_id_from_path(path: &Path) -> String {
    // 1. Try git remote origin URL
    if let Some(id) = project_id_from_git_remote(path) {
        return id;
    }

    //   2. Git repo but no remote — use repo root dir name
    if let Some(id) = project_id_from_git_root(path) {
        return id;
    }

    let folder = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let folder = sanitize(&folder);

    //   3. Not a git repo — use `git config --global user.name` + folder name
    if let Some(username) = git_global_username() {
        return format!("{}-{}", sanitize(&username), folder);
    }

    folder

}

// ── Strategy 1: git remote origin ────────────────────────────────────────────

fn project_id_from_git_remote(cwd: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        return None;
    }

    Some(normalize_git_url(&url))
}

/// Normalize any git remote URL to a stable `repo` id.
///
/// Handles:
///   https://github.com/org/repo.git  → org-repo
///   git@github.com:org/repo.git      → org-repo
///   https://gitlab.com/org/sub/repo  → subok -repo
fn normalize_git_url(url: &str) -> String {
    let url = url.trim_end_matches(".git").trim();


    let mut parts = url.split(&['/', ':'][..]).filter(|s| !s.is_empty()).rev();

    let repo = parts.next().unwrap_or("project");
    let org  = parts.next().unwrap_or("unknown");

    sanitize(&format!("{}-{}", org, repo))

}

// ── Strategy 2: git root dir name + global username ──────────────────────────

fn project_id_from_git_root(cwd: &Path) -> Option<String> {
    let output: std::process::Output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let root_path = std::path::Path::new(&root);
    let dir_name = sanitize(root_path.file_name()?.to_str()?);

    if let Some(username) = git_global_username() {
        Some(format!("{}-{}", sanitize(&username), dir_name))
    } else {
        Some(dir_name)
    }
}

// ── Strategy 3 helper: global git username ────────────────────────────────────

fn git_global_username() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", "--global", "user.name"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

// ── Shared sanitizer ──────────────────────────────────────────────────────────

/// Make a string safe for use as a path component and cloud project id.
/// Keeps alphanumerics, `.`, `_`; replaces everything else with `-`;
/// collapses consecutive dashes; strips leading/trailing dashes.
fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_' || ch == '.';
        if ok {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

// ── Directory helpers ─────────────────────────────────────────────────────────

pub fn project_dir(base: &Path, project_id: &str) -> PathBuf {
    base.join("projects").join(project_id)
}
