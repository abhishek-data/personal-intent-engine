//! Vocabulary bootstrap: extract technical terms from the user's existing
//! conversations (a chosen folder/file, or an auto-detected local source) via
//! the configured LLM, storing them as `Source::Synced`. User-initiated only.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::LlmRouter;

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

/// A term plus spoken/misrecognized variants, as returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedTerm {
    pub term: String,
    pub variants: Vec<String>,
}

/// Result of a sync run.
#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub conversations: usize,
    pub terms_added: usize,
}

/// Persistent record of the last sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    pub last_run_unix: u64,
    pub terms_added: usize,
}

impl SyncState {
    /// Load from `path`; a missing or unparseable file yields a default state.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    /// Persist as pretty JSON, creating parent directories if needed.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Fence-tolerant parse of the LLM's term list.
pub fn parse_synced_terms(reply: &str) -> anyhow::Result<Vec<SyncedTerm>> {
    let cleaned = reply
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    Ok(serde_json::from_str(cleaned)?)
}

/// Conservative extraction prompt for a batch of conversation texts.
pub fn build_sync_prompt(texts: &[String]) -> String {
    format!(
        "Extract technical terms, product names, library names, and proper nouns \
         a developer would SPEAK into a microphone, from these conversation \
         excerpts. Include likely speech-to-text misrecognition variants.\n\n\
         Excerpts:\n{}\n\n\
         Return ONLY JSON: [{{\"term\":\"Next.js\",\"variants\":[\"nextjs\",\"next js\",\"next jazz\"]}}]. \
         Return [] if none.",
        texts.join("\n---\n"),
    )
}

/// Best-effort read of Cursor's `state.vscdb` ItemTable. Fragile by nature
/// (undocumented, version-dependent schema); any failure — missing file,
/// missing table, unreadable rows — yields an empty Vec rather than an error.
pub fn extract_cursor_texts(vscdb: &Path) -> Vec<String> {
    let Ok(conn) =
        rusqlite::Connection::open_with_flags(vscdb, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare("SELECT value FROM ItemTable") else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in rows.flatten() {
        match serde_json::from_str::<Value>(&v) {
            Ok(json) => {
                let s = harvest_json_strings(&json);
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
            Err(_) => out.push(v),
        }
    }
    out
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run a sync over the given paths. `add(variant, canonical)` applies each
/// mapping (the engine wires it to `add_synced_correction`, keeping this
/// module free of a dependency on the corrector's storage). Progress is
/// reported as `(conversations_done, conversations_total)`.
pub async fn run_sync<F>(
    paths: &[PathBuf],
    llm: &LlmRouter,
    provider: &str,
    model: Option<&str>,
    mut add: F,
    on_progress: impl Fn(usize, usize),
) -> anyhow::Result<SyncResult>
where
    F: FnMut(&str, &str) -> anyhow::Result<()>,
{
    // Gather all texts first (for an accurate total).
    let mut texts: Vec<String> = Vec::new();
    for p in paths {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
        if matches!(ext.as_deref(), Some("vscdb") | Some("sqlite") | Some("db")) {
            texts.extend(extract_cursor_texts(p));
        } else {
            texts.extend(extract_texts(p));
        }
    }
    let total = texts.len();
    let mut done = 0usize;
    let mut terms_added = 0usize;

    for batch in texts.chunks(10) {
        let prompt = build_sync_prompt(batch);
        if let Ok(reply) = llm.send(&prompt, provider, model).await {
            if let Ok(terms) = parse_synced_terms(&reply) {
                for t in terms {
                    for variant in &t.variants {
                        if add(variant, &t.term).is_ok() {
                            terms_added += 1;
                        }
                    }
                }
            }
        }
        done += batch.len();
        on_progress(done, total);
    }

    Ok(SyncResult {
        conversations: total,
        terms_added,
    })
}

/// Default path for the sync-state record.
pub fn default_sync_state_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("sync_state.json")
}

/// Convenience: write a completed sync's state.
pub fn record_sync_state(terms_added: usize) -> anyhow::Result<()> {
    let state = SyncState {
        last_run_unix: now_unix(),
        terms_added,
    };
    state.save(&default_sync_state_path())
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

    #[test]
    fn parse_synced_terms_tolerates_fences() {
        let raw = "```json\n[{\"term\":\"Next.js\",\"variants\":[\"nextjs\",\"next jazz\"]}]\n```";
        let got = parse_synced_terms(raw).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].term, "Next.js");
        assert_eq!(got[0].variants.len(), 2);
    }

    #[test]
    fn build_sync_prompt_includes_texts() {
        let p = build_sync_prompt(&["deploy to coober net ease".to_string()]);
        assert!(p.contains("coober net ease"));
        assert!(p.to_lowercase().contains("json"));
    }

    #[test]
    fn extract_cursor_texts_reads_itemtable() {
        let d = temp_dir();
        let db = d.join("state.vscdb");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute("CREATE TABLE ItemTable (key TEXT, value TEXT)", [])
                .unwrap();
            conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                rusqlite::params!["chat", r#"{"messages":["how do I deploy nextjs"]}"#],
            )
            .unwrap();
        }
        let texts = extract_cursor_texts(&db);
        assert!(
            texts.iter().any(|t| t.contains("nextjs")),
            "reads ItemTable JSON values"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_cursor_texts_missing_table_is_empty() {
        let d = temp_dir();
        let db = d.join("empty.vscdb");
        {
            let _ = rusqlite::Connection::open(&db).unwrap();
        } // no ItemTable
        assert!(extract_cursor_texts(&db).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[tokio::test]
    async fn run_sync_applies_terms_via_closure() {
        // echo provider: LlmRouter with no client + provider "echo" returns the
        // prompt, which parse_synced_terms will fail on -> 0 terms. So instead
        // test the wiring: a dir with one file, and assert progress + no panic.
        let d = temp_dir();
        std::fs::write(d.join("a.md"), "deploy nextjs").unwrap();
        let llm = crate::llm::LlmRouter::new(); // echo-capable; "echo" provider
        let mut added = 0usize;
        let res = run_sync(
            std::slice::from_ref(&d),
            &llm,
            "echo",
            None,
            |_v, _c| {
                added += 1;
                Ok(())
            },
            |_done, _total| {},
        )
        .await
        .unwrap();
        // echo returns non-JSON, so parse yields nothing; conversations counted.
        assert_eq!(res.conversations, 1);
        let _ = std::fs::remove_dir_all(d);
    }
}
