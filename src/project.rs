use std::path::{Path, PathBuf};

/// Derive a stable project id from an absolute path.
/// Matches Cursor's `~/.cursor/projects/<id>` convention on macOS:
/// `/Users/alice/dev/foo` -> `Users-alice-dev-foo`
pub fn project_id_from_path(path: &Path) -> String {
    // Cursor appears to drop the leading path separator and join components with '-'.
    let mut s = String::new();
    for (i, comp) in path.components().filter_map(|c| c.as_os_str().to_str()).enumerate() {
        // Skip empty / root-like segments.
        if comp.is_empty() {
            continue;
        }
        if i > 0 && !s.is_empty() {
            s.push('-');
        }
        s.push_str(&sanitize_component(comp));
    }
    if s.starts_with('-') {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

fn sanitize_component(comp: &str) -> String {
    // Keep it simple and filesystem-safe.
    let mut out = String::with_capacity(comp.len());
    let mut last_dash = false;
    for ch in comp.chars() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_' || ch == '.';
        let mapped = if ok { ch } else { '-' };
        if mapped == '-' {
            if !last_dash {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(mapped);
            last_dash = false;
        }
    }
    out.trim_matches('-').to_string()
}

pub fn project_dir(base: &Path, project_id: &str) -> PathBuf {
    base.join("projects").join(project_id)
}

