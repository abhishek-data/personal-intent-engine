//! Local SQLite store of recording history. One row per recording, capturing
//! the full pipeline outcome so the data is useful for later analysis.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rusqlite::{params, Connection, Row};
use serde::Serialize;

/// A stored history row.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: i64,
    pub transcript: String,
    pub objective: Option<String>,
    pub conversation_type: Option<String>,
    pub confidence: Option<String>,
    pub optimized_prompt: Option<String>,
    pub estimated_tokens: Option<i64>,
    pub mode: Option<String>,
    pub language: Option<String>,
}

/// A row to insert (id + created_at are assigned by the store).
#[derive(Debug, Clone, Default)]
pub struct NewEntry {
    pub transcript: String,
    pub objective: Option<String>,
    pub conversation_type: Option<String>,
    pub confidence: Option<String>,
    pub optimized_prompt: Option<String>,
    pub estimated_tokens: Option<i64>,
    pub mode: Option<String>,
    pub language: Option<String>,
}

const COLUMNS: &str = "id, created_at, transcript, objective, conversation_type, \
    confidence, optimized_prompt, estimated_tokens, mode, language";

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    /// Open (creating parent dirs and the file if needed) and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let store = Self {
            conn: Connection::open(path)?,
        };
        store.migrate()?;
        Ok(store)
    }

    /// In-memory store for tests.
    pub fn open_in_memory() -> Result<Self> {
        let store = Self {
            conn: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Idempotent schema migration keyed on `PRAGMA user_version`.
    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history (
                    id                INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at        INTEGER NOT NULL,
                    transcript        TEXT    NOT NULL,
                    objective         TEXT,
                    conversation_type TEXT,
                    confidence        TEXT,
                    optimized_prompt  TEXT,
                    estimated_tokens  INTEGER,
                    mode              TEXT,
                    language          TEXT
                 );
                 CREATE INDEX IF NOT EXISTS idx_history_created_at
                     ON history(created_at DESC);
                 PRAGMA user_version = 1;",
            )?;
        }
        Ok(())
    }

    /// Insert a row, then prune to the newest `limit` rows. Returns the new id.
    pub fn add(&self, entry: NewEntry, limit: usize) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO history
             (created_at, transcript, objective, conversation_type, confidence,
              optimized_prompt, estimated_tokens, mode, language)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                now_unix(),
                entry.transcript,
                entry.objective,
                entry.conversation_type,
                entry.confidence,
                entry.optimized_prompt,
                entry.estimated_tokens,
                entry.mode,
                entry.language,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune(limit)?;
        Ok(id)
    }

    fn prune(&self, limit: usize) -> Result<()> {
        let limit = limit.max(1) as i64;
        self.conn.execute(
            "DELETE FROM history
             WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            params![limit],
        )?;
        Ok(())
    }

    /// Newest-first list, optionally filtered by a substring of the transcript.
    pub fn list(&self, query: Option<&str>, limit: usize) -> Result<Vec<HistoryEntry>> {
        let limit = limit as i64;
        let rows = match query {
            Some(q) if !q.is_empty() => {
                let sql = format!(
                    "SELECT {COLUMNS} FROM history
                     WHERE transcript LIKE '%' || ?1 || '%'
                     ORDER BY id DESC LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let mapped = stmt.query_map(params![q, limit], map_row)?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
            _ => {
                let sql = format!("SELECT {COLUMNS} FROM history ORDER BY id DESC LIMIT ?1");
                let mut stmt = self.conn.prepare(&sql)?;
                let mapped = stmt.query_map(params![limit], map_row)?;
                mapped.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(rows)
    }

    /// Fetch a single row by id.
    pub fn get(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let sql = format!("SELECT {COLUMNS} FROM history WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![id], map_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?)
    }
}

fn map_row(r: &Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        created_at: r.get(1)?,
        transcript: r.get(2)?,
        objective: r.get(3)?,
        conversation_type: r.get(4)?,
        confidence: r.get(5)?,
        optimized_prompt: r.get(6)?,
        estimated_tokens: r.get(7)?,
        mode: r.get(8)?,
        language: r.get(9)?,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> NewEntry {
        NewEntry {
            transcript: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn add_then_list_is_newest_first() {
        let store = HistoryStore::open_in_memory().unwrap();
        store.add(entry("first"), 10).unwrap();
        store.add(entry("second"), 10).unwrap();

        let all = store.list(None, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].transcript, "second");
        assert_eq!(all[1].transcript, "first");
    }

    #[test]
    fn hard_cap_deletes_oldest() {
        let store = HistoryStore::open_in_memory().unwrap();
        for i in 0..5 {
            store.add(entry(&format!("row{i}")), 3).unwrap();
        }
        assert_eq!(store.count().unwrap(), 3);
        let all = store.list(None, 10).unwrap();
        assert_eq!(all[0].transcript, "row4");
        assert_eq!(all[2].transcript, "row2");
    }

    #[test]
    fn lowering_limit_prunes_on_next_add() {
        let store = HistoryStore::open_in_memory().unwrap();
        for i in 0..4 {
            store.add(entry(&format!("row{i}")), 10).unwrap();
        }
        assert_eq!(store.count().unwrap(), 4);
        store.add(entry("row4"), 2).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn list_query_filters_by_substring() {
        let store = HistoryStore::open_in_memory().unwrap();
        store.add(entry("draft an email"), 10).unwrap();
        store.add(entry("write some code"), 10).unwrap();

        let hits = store.list(Some("email"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].transcript, "draft an email");
    }

    #[test]
    fn delete_removes_one_clear_empties() {
        let store = HistoryStore::open_in_memory().unwrap();
        let id = store.add(entry("keep me? no"), 10).unwrap();
        store.add(entry("keep me"), 10).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().transcript, "keep me? no");
        store.delete(id).unwrap();
        assert!(store.get(id).unwrap().is_none());
        assert_eq!(store.count().unwrap(), 1);
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn migrate_is_idempotent() {
        let store = HistoryStore::open_in_memory().unwrap();
        store.add(entry("row"), 10).unwrap();
        // Running the migration again must not error or wipe data.
        store.migrate().unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    /// Guards the `add` -> `map_row` column ordering: every field on a fully
    /// populated entry must round-trip through insert + list unchanged.
    #[test]
    fn full_entry_round_trips_all_fields() {
        let store = HistoryStore::open_in_memory().unwrap();
        let new_entry = NewEntry {
            transcript: "full round trip".to_string(),
            objective: Some("objective text".to_string()),
            conversation_type: Some("Question".to_string()),
            confidence: Some("High".to_string()),
            optimized_prompt: Some("optimized prompt text".to_string()),
            estimated_tokens: Some(42),
            mode: Some("Fast".to_string()),
            language: Some("en".to_string()),
        };
        store.add(new_entry, 10).unwrap();

        let all = store.list(None, 10).unwrap();
        assert_eq!(all.len(), 1);
        let got = &all[0];
        assert_eq!(got.transcript, "full round trip");
        assert_eq!(got.objective.as_deref(), Some("objective text"));
        assert_eq!(got.conversation_type.as_deref(), Some("Question"));
        assert_eq!(got.confidence.as_deref(), Some("High"));
        assert_eq!(got.optimized_prompt.as_deref(), Some("optimized prompt text"));
        assert_eq!(got.estimated_tokens, Some(42));
        assert_eq!(got.mode.as_deref(), Some("Fast"));
        assert_eq!(got.language.as_deref(), Some("en"));
    }

    #[test]
    fn zero_limit_keeps_at_least_one_row() {
        let store = HistoryStore::open_in_memory().unwrap();
        store.add(entry("row"), 0).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn persists_across_reopen() {
        let path = std::env::temp_dir().join(format!("pie-hist-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let store = HistoryStore::open(&path).unwrap();
            store.add(entry("persisted"), 10).unwrap();
        }
        {
            let store = HistoryStore::open(&path).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            assert_eq!(store.list(None, 10).unwrap()[0].transcript, "persisted");
        }
        let _ = std::fs::remove_file(&path);
    }
}
