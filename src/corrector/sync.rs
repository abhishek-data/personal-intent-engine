//! Vocabulary bootstrap: extract technical terms from the user's existing
//! conversations (a chosen folder/file, or an auto-detected local source) via
//! the configured LLM, storing them as `Source::Synced`. User-initiated only.

use std::path::Path;

use serde_json::Value;

/// Recursively collect every JSON string value, newline-joined. Generically
/// captures message text from exports like ChatGPT's `conversations.json`
/// without hard-coding a schema.
pub fn harvest_json_strings(v: &Value) -> String {
    let mut out = Vec::new();
    harvest_into(v, &mut out);
    out.join("\n")
}

fn harvest_into(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|e| harvest_into(e, out)),
        Value::Object(o) => o.values().for_each(|e| harvest_into(e, out)),
        _ => {}
    }
}

const TEXT_EXTS: [&str; 4] = ["md", "txt", "json", "markdown"];

/// Extract conversation-ish text blocks from a chosen path (folder or file).
/// Unreadable/unknown inputs yield an empty Vec — never an error.
pub fn extract_texts(path: &Path) -> Vec<String> {
    if path.is_dir() {
        let mut out = Vec::new();
        walk_files(path, &mut out, 0);
        out
    } else if path.is_file() {
        extract_one(path).into_iter().collect()
    } else {
        Vec::new()
    }
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

fn extract_one(path: &Path) -> Option<String> {
    let ext = ext_lower(path)?;
    if !TEXT_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    if ext == "json" {
        let v: Value = serde_json::from_str(&raw).ok()?;
        Some(harvest_json_strings(&v))
    } else {
        Some(raw)
    }
}

fn walk_files(dir: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, out, depth + 1);
        } else if let Some(text) = extract_one(&p) {
            if !text.trim().is_empty() {
                out.push(text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);
    fn temp_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "pie-sync-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn harvest_json_strings_collects_nested_values() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"a":"deploy to kubernetes","b":{"c":["next.js","nginx"]},"n":42}"#,
        )
        .unwrap();
        let got = harvest_json_strings(&v);
        assert!(got.contains("deploy to kubernetes"));
        assert!(got.contains("next.js"));
        assert!(got.contains("nginx"));
        assert!(!got.contains("42"), "numbers are not harvested");
    }

    #[test]
    fn extract_texts_reads_dir_of_md_txt_json() {
        let d = temp_dir();
        std::fs::write(d.join("a.md"), "I use Terraform daily").unwrap();
        std::fs::write(d.join("b.txt"), "spin up on AWS").unwrap();
        std::fs::write(d.join("c.json"), r#"{"msg":"scale with Kubernetes"}"#).unwrap();
        std::fs::write(d.join("ignore.png"), b"\x89PNG").unwrap();
        let mut texts = extract_texts(&d);
        texts.sort();
        assert_eq!(texts.len(), 3, "3 recognized files, png ignored");
        assert!(texts.iter().any(|t| t.contains("Terraform")));
        assert!(texts.iter().any(|t| t.contains("Kubernetes")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_texts_single_json_file() {
        let d = temp_dir();
        let f = d.join("conversations.json");
        std::fs::write(&f, r#"[{"content":"deploy nextjs on vercel"}]"#).unwrap();
        let texts = extract_texts(&f);
        assert_eq!(texts.len(), 1);
        assert!(texts[0].contains("nextjs"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_texts_missing_path_is_empty() {
        assert!(extract_texts(std::path::Path::new("/no/such/path/xyz")).is_empty());
    }
}
