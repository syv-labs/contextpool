pub fn extract_text_from_jsonl(jsonl: &str) -> String {
    let mut out = String::new();
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(s) = extract_text_from_value(&v) {
                out.push_str(&s);
                out.push('\n');
                continue;
            }
        }
        // Fallback: keep raw line if unknown format
        out.push_str(line);
        out.push('\n');
    }
    out
}

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

fn extract_text_from_value(v: &serde_json::Value) -> Option<String> {
    // We don't assume a single schema; try a few common shapes.
    let keys = ["content", "text", "message", "body"];
    for k in keys {
        if let Some(val) = v.get(k) {
            if let Some(s) = val.as_str() {
                return Some(s.to_string());
            }
        }
    }

    // Sometimes messages are nested: { message: { content: "..." } }
    if let Some(m) = v.get("message") {
        if let Some(s) = m.as_str() {
            return Some(s.to_string());
        }
        if let Some(obj) = m.as_object() {
            for k in keys {
                if let Some(val) = obj.get(k).and_then(|x| x.as_str()) {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
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
            // Prefer a few common "message content" keys to keep the output tighter.
            // If present, record those first, then continue deep-walk.
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

