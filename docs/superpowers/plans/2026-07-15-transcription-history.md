# Transcription History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist every recording to a local SQLite database and add a searchable History tab to the desktop app, plus pin the Record button to the bottom of the Record view.

**Architecture:** A new `HistoryStore` in the core `pie-engine` lib wraps a `rusqlite` connection (bundled SQLite) with schema migrations. The Tauri app opens one store at startup, records an entry at the single `transcribe_and_process` chokepoint (covers hotkey + in-app Record), and exposes list/delete/clear/paste commands to a new Svelte `HistoryView`.

**Tech Stack:** Rust, `rusqlite` 0.32 (feature `bundled`), Tauri 2, Svelte 5 (runes), Vite.

## Global Constraints

- No new UI libraries — styling is pure CSS in `ui/src/app.css`, matching the existing dark/indigo design system.
- New Rust dependency limited to `rusqlite = { version = "0.32", features = ["bundled"] }` in the **root** `Cargo.toml` (the `pie-engine` crate). No `uuid`, no `chrono`.
- History writes are best-effort: a failure must log a warning and never fail the recording/paste flow.
- Retention is a hard cap: setting `history_limit`, default **10**; older rows deleted on insert.
- Timestamps are unix seconds (`i64`); ids are SQLite `AUTOINCREMENT` (`i64`).
- Rust crate name as imported from the app is `pie_engine`.

---

### Task 1: `HistoryStore` core module

**Files:**
- Modify: `Cargo.toml` (add rusqlite dependency)
- Modify: `src/lib.rs` (add `pub mod history;`)
- Create: `src/history/mod.rs`

**Interfaces:**
- Produces:
  - `pie_engine::history::HistoryEntry` — `{ id: i64, created_at: i64, transcript: String, objective: Option<String>, conversation_type: Option<String>, confidence: Option<String>, optimized_prompt: Option<String>, estimated_tokens: Option<i64>, mode: Option<String>, language: Option<String> }`, derives `Debug, Clone, serde::Serialize`.
  - `pie_engine::history::NewEntry` — same fields minus `id`/`created_at`, derives `Debug, Clone, Default`.
  - `pie_engine::history::HistoryStore` with:
    - `open(path: &std::path::Path) -> anyhow::Result<Self>`
    - `open_in_memory() -> anyhow::Result<Self>`
    - `add(&self, entry: NewEntry, limit: usize) -> anyhow::Result<i64>`
    - `list(&self, query: Option<&str>, limit: usize) -> anyhow::Result<Vec<HistoryEntry>>`
    - `get(&self, id: i64) -> anyhow::Result<Option<HistoryEntry>>`
    - `delete(&self, id: i64) -> anyhow::Result<()>`
    - `clear(&self) -> anyhow::Result<()>`
    - `count(&self) -> anyhow::Result<i64>`

- [ ] **Step 1: Add the rusqlite dependency**

In `Cargo.toml`, under `[dependencies]` (next to `dirs = "6"`), add:

```toml
# Local SQLite store for transcription history (bundled = no system dep).
rusqlite = { version = "0.32", features = ["bundled"] }
```

- [ ] **Step 2: Register the module**

In `src/lib.rs`, add to the module list (after `pub mod audio;`):

```rust
pub mod history;
```

- [ ] **Step 3: Write the module with failing tests**

Create `src/history/mod.rs`:

```rust
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
        self.conn.execute(
            "DELETE FROM history
             WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            params![limit as i64],
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
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p pie-engine history --`
Expected: PASS — 6 tests (`add_then_list_is_newest_first`, `hard_cap_deletes_oldest`, `lowering_limit_prunes_on_next_add`, `list_query_filters_by_substring`, `delete_removes_one_clear_empties`, `migrate_is_idempotent`).

Note: the first build compiles bundled SQLite and may take a minute.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/history/mod.rs
git commit -m "feat: SQLite-backed HistoryStore in core lib"
```

---

### Task 2: Add `history_limit` to app settings

**Files:**
- Modify: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `Settings.history_limit: usize` (default 10). Already-persisted `settings.json` files without the field load fine because the struct is `#[serde(default)]`.

- [ ] **Step 1: Add the field to the struct**

In `src-tauri/src/settings.rs`, in the `pub struct Settings { ... }` block, add after `paste_output`:

```rust
    /// Max number of recordings kept in the history store (hard cap).
    pub history_limit: usize,
```

- [ ] **Step 2: Set the default**

In the `impl Default for Settings`, add after `paste_output: "transcript".to_string(),`:

```rust
            history_limit: 10,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p pie-desktop 2>&1 | tail -5`
Expected: builds (warnings ok, no errors).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "feat: history_limit setting (default 10)"
```

---

### Task 3: Add the missing `copy_to_clipboard` command

The frontend already calls `invoke("copy_to_clipboard", { text })` ([App.svelte:98](../../../ui/src/App.svelte)) but no such command is registered, so Copy currently fails. History's Copy needs it too.

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces: Tauri command `copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String>`.

- [ ] **Step 1: Add the command**

In `src-tauri/src/main.rs`, near the other `#[tauri::command]` functions (e.g. after `send_to_llm`), add:

```rust
#[tauri::command]
fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Failed to copy: {e}"))
}
```

- [ ] **Step 2: Register it**

In the `tauri::generate_handler![ ... ]` list, add `copy_to_clipboard,` after `send_to_llm,`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p pie-desktop 2>&1 | tail -5`
Expected: builds, no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "fix: register copy_to_clipboard command"
```

---

### Task 4: Open the store, capture on every recording

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `pie_engine::history::{HistoryStore, NewEntry}` (Task 1), `Settings.history_limit` (Task 2).
- Produces: `AppState.history: Mutex<HistoryStore>`; a `pie://history-changed` event emitted after each successful capture.

- [ ] **Step 1: Import the store**

In `src-tauri/src/main.rs`, after `use pie_engine::stt::{...};`, add:

```rust
use pie_engine::history::{HistoryStore, NewEntry};
```

- [ ] **Step 2: Add the field to `AppState`**

In `struct AppState { ... }`, add:

```rust
    /// Local SQLite history of recordings.
    history: Mutex<HistoryStore>,
```

- [ ] **Step 3: Open the store in `setup` and manage it**

In the `.setup(|app| { ... })` closure, before `app.manage(AppState { ... })`, add:

```rust
            let history_path = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("pie")
                .join("history.db");
            let history = HistoryStore::open(&history_path).unwrap_or_else(|e| {
                log::error!("Failed to open history DB ({e}); using in-memory");
                HistoryStore::open_in_memory().expect("in-memory history must open")
            });
```

Then add to the `AppState { ... }` initializer (after `engine: tokio::sync::Mutex::new(engine),`):

```rust
                history: Mutex::new(history),
```

- [ ] **Step 4: Add the `opt` helper**

In `src-tauri/src/main.rs`, in the `/* ─── helpers ─── */` section, add:

```rust
/// Map an empty string to `None` so blank optimizer fields aren't stored.
fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}
```

- [ ] **Step 5: Record the entry in `transcribe_and_process`**

In `transcribe_and_process`, replace the final `Ok(Outcome { ... })` block:

```rust
    Ok(Outcome {
        transcript,
        objective: result.intent.objective,
        conversation_type: format!("{:?}", result.intent.conversation_type),
        confidence: format!("{:?}", result.intent.confidence),
        optimized_prompt: result.optimized_prompt,
        estimated_tokens: result.estimated_tokens,
        mode: format!("{:?}", result.mode),
    })
```

with:

```rust
    let outcome = Outcome {
        transcript,
        objective: result.intent.objective,
        conversation_type: format!("{:?}", result.intent.conversation_type),
        confidence: format!("{:?}", result.intent.confidence),
        optimized_prompt: result.optimized_prompt,
        estimated_tokens: result.estimated_tokens,
        mode: format!("{:?}", result.mode),
    };

    // Best-effort history capture — never fail the recording over it.
    {
        let entry = NewEntry {
            transcript: outcome.transcript.clone(),
            objective: opt(&outcome.objective),
            conversation_type: opt(&outcome.conversation_type),
            confidence: opt(&outcome.confidence),
            optimized_prompt: opt(&outcome.optimized_prompt),
            estimated_tokens: Some(outcome.estimated_tokens as i64),
            mode: opt(&outcome.mode),
            language: opt(&settings.language),
        };
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        match history.add(entry, settings.history_limit) {
            Ok(_) => emit_event(app, "pie://history-changed", ()),
            Err(e) => log::warn!("Failed to record history: {e}"),
        }
    }

    Ok(outcome)
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p pie-desktop 2>&1 | tail -5`
Expected: builds, no errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: capture history on every recording"
```

---

### Task 5: History Tauri commands

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `AppState.history` (Task 4), `paste::paste_text` + `EnigoState` (existing), `pie_engine::history::HistoryEntry`.
- Produces: commands `list_history`, `delete_history_entry`, `clear_history`, `paste_history_entry`.

- [ ] **Step 1: Add the commands**

In `src-tauri/src/main.rs`, near the other commands, add:

```rust
#[tauri::command]
fn list_history(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<pie_engine::history::HistoryEntry>, String> {
    let limit = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .history_limit;
    let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
    history
        .list(query.as_deref(), limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history.delete(id).map_err(|e| e.to_string())?;
    }
    emit_event(&app, "pie://history-changed", ());
    Ok(())
}

#[tauri::command]
fn clear_history(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history.clear().map_err(|e| e.to_string())?;
    }
    emit_event(&app, "pie://history-changed", ());
    Ok(())
}

// Synchronous: it blocks with sleeps + keystroke simulation. Tauri runs sync
// commands on its own thread pool, so this won't stall the async runtime.
#[tauri::command]
fn paste_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    enigo: State<'_, EnigoState>,
    id: i64,
) -> Result<(), String> {
    let text = {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history
            .get(id)
            .map_err(|e| e.to_string())?
            .map(|r| r.transcript)
            .ok_or_else(|| "History entry not found".to_string())?
    };

    // Hide the main window so focus returns to the previously active app,
    // then paste into it (same mechanism as the hotkey flow).
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
    paste::paste_text(&app, &enigo, &text)
}
```

- [ ] **Step 2: Register the commands**

In `tauri::generate_handler![ ... ]`, add after `copy_to_clipboard,`:

```rust
            list_history,
            delete_history_entry,
            clear_history,
            paste_history_entry,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p pie-desktop 2>&1 | tail -5`
Expected: builds, no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: history list/delete/clear/paste commands"
```

---

### Task 6: Retention control in Settings

**Files:**
- Create: `ui/src/lib/HistorySettings.svelte`
- Modify: `ui/src/App.svelte` (add `history_limit` to the settings object; render the new component in the settings view)

**Interfaces:**
- Consumes: `settings` object + `onSave` (existing settings-component contract).
- Produces: a bound number input for `settings.history_limit`.

- [ ] **Step 1: Add `history_limit` to the frontend settings object**

In `ui/src/App.svelte`, in the `let settings = $state({ ... })` initializer, add after `paste_output: "transcript",`:

```javascript
    history_limit: 10,
```

- [ ] **Step 2: Create the settings component**

Create `ui/src/lib/HistorySettings.svelte`:

```svelte
<script>
  let { settings, onSave } = $props();
</script>

<section class="group">
  <span class="group-eyebrow">History</span>
  <div class="field">
    <label for="history-limit">Recordings to keep</label>
    <input
      id="history-limit"
      type="number"
      min="1"
      max="1000"
      bind:value={settings.history_limit}
      onblur={onSave}
    />
    <p class="caption">Older recordings beyond this count are deleted automatically.</p>
  </div>
</section>
```

- [ ] **Step 3: Render it in the settings view**

In `ui/src/App.svelte`, import it with the other lib imports:

```javascript
  import HistorySettings from "./lib/HistorySettings.svelte";
```

Then in the `{:else if view === "settings"}` block, add after `<HotkeyRecorder ... />`:

```svelte
    <HistorySettings {settings} onSave={save} />
```

- [ ] **Step 4: Verify the build**

Run: `npm run build --prefix ui 2>&1 | tail -5`
Expected: `✓ built` with no errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/App.svelte ui/src/lib/HistorySettings.svelte
git commit -m "feat: history retention control in settings"
```

---

### Task 7: History tab and view

**Files:**
- Create: `ui/src/lib/HistoryView.svelte`
- Modify: `ui/src/App.svelte` (add the tab, route it, pass handlers)
- Modify: `ui/src/app.css` (history list styling)

**Interfaces:**
- Consumes: commands `list_history`, `delete_history_entry`, `clear_history`, `paste_history_entry`, `copy_to_clipboard`; event `pie://history-changed`.

- [ ] **Step 1: Create the view**

Create `ui/src/lib/HistoryView.svelte`:

```svelte
<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let entries = $state([]);
  let query = $state("");
  let error = $state("");
  let searchTimer;

  async function refresh() {
    try {
      entries = await invoke("list_history", { query: query || null });
    } catch (e) { error = String(e); }
  }

  function onSearch() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(refresh, 150);
  }

  async function copy(text) {
    try { await invoke("copy_to_clipboard", { text }); }
    catch (e) { error = String(e); }
  }

  async function paste(id) {
    try { await invoke("paste_history_entry", { id }); }
    catch (e) { error = String(e); }
  }

  async function remove(id) {
    try { await invoke("delete_history_entry", { id }); await refresh(); }
    catch (e) { error = String(e); }
  }

  async function clearAll() {
    if (!confirm("Delete all history?")) return;
    try { await invoke("clear_history"); await refresh(); }
    catch (e) { error = String(e); }
  }

  function relTime(unixSeconds) {
    const s = Math.max(0, Math.floor(Date.now() / 1000 - unixSeconds));
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.floor(s / 60)}m ago`;
    if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
    return `${Math.floor(s / 86400)}d ago`;
  }

  onMount(() => {
    refresh();
    let unlisten;
    listen("pie://history-changed", () => refresh()).then((u) => { unlisten = u; });
    return () => { if (unlisten) unlisten(); };
  });
</script>

<div class="history">
  <input
    class="history-search"
    placeholder="Search transcripts…"
    bind:value={query}
    oninput={onSearch}
  />

  {#if error}
    <p class="caption" style="color:var(--danger)">{error}</p>
  {/if}

  {#if entries.length === 0}
    <p class="history-empty">No recordings yet. Press your hotkey or record to start.</p>
  {:else}
    <ul class="history-list">
      {#each entries as e (e.id)}
        <li class="history-item">
          <div class="history-text">{e.transcript}</div>
          <div class="history-meta">
            <span class="history-time">{relTime(e.created_at)}</span>
            <div class="history-actions">
              <button class="text-btn" onclick={() => copy(e.transcript)}>Copy</button>
              <button class="text-btn" onclick={() => paste(e.id)}>Paste</button>
              <button class="text-btn danger" onclick={() => remove(e.id)}>Delete</button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
    <button class="text-btn danger history-clear" onclick={clearAll}>Clear all</button>
  {/if}
</div>
```

- [ ] **Step 2: Wire the tab into `App.svelte`**

Import at the top with the other lib imports:

```javascript
  import HistoryView from "./lib/HistoryView.svelte";
```

Add to the `TABS` array (after the `record` entry, so History sits next to Record):

```javascript
    { id: "history", label: "History" },
```

Add a route branch — in the view `{#if}`/`{:else if}` chain, add before `{:else if view === "settings"}`:

```svelte
  {:else if view === "history"}
    <HistoryView />
```

- [ ] **Step 3: Add the styles**

Append to `ui/src/app.css`:

```css
/* ─── History ─── */
.history { display: flex; flex-direction: column; gap: var(--space-3); }
.history-search {
  width: 100%;
  padding: 9px 12px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--fg);
  font: inherit;
  font-size: 13px;
}
.history-search:focus-visible { outline: none; border-color: var(--accent); box-shadow: var(--ring); }
.history-empty { color: var(--fg3); font-size: 13px; text-align: center; padding: 40px 0; }
.history-list { list-style: none; display: flex; flex-direction: column; gap: var(--space-2); }
.history-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.history-text {
  font-size: 13px;
  color: var(--fg);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.history-meta { display: flex; align-items: center; justify-content: space-between; }
.history-time { font-size: 11px; color: var(--fg3); }
.history-actions { display: flex; gap: var(--space-2); }
.text-btn.danger { color: var(--danger); }
.history-clear { align-self: flex-end; margin-top: var(--space-2); }
```

- [ ] **Step 4: Verify the build**

Run: `npm run build --prefix ui 2>&1 | tail -5`
Expected: `✓ built` with no errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/App.svelte ui/src/lib/HistoryView.svelte ui/src/app.css
git commit -m "feat: searchable history tab"
```

---

### Task 8: Pin the Record button to the bottom

Restructure the Record view so the record button + state sit in a fixed bottom bar, with transcript/result content scrolling above (OpenSuperWhisper-style).

**Files:**
- Modify: `ui/src/lib/RecordingView.svelte`
- Modify: `ui/src/app.css`

- [ ] **Step 1: Restructure the markup**

Replace the entire contents of `ui/src/lib/RecordingView.svelte` with:

```svelte
<script>
  let { recState, outcome, llmResponse, llmBusy, hotkey, stateLabel, onToggle, onCancel, onSend, onCopy } = $props();
</script>

<div class="record-view">
  <div class="record-scroll">
    {#if outcome}
      <section class="result">
        <div class="result-step">
          <span class="eyebrow">Heard</span>
          <p class="transcript">{outcome.transcript}</p>
        </div>

        <div class="result-step">
          <span class="eyebrow">Understood</span>
          <div class="chips">
            <span class="chip">{outcome.conversation_type}</span>
            <span class="chip">{outcome.confidence} confidence</span>
            {#if outcome.objective}
              <span class="chip objective">{outcome.objective}</span>
            {/if}
          </div>
        </div>

        <div class="result-step">
          <div class="step-head">
            <span class="eyebrow">Optimized prompt</span>
            <span class="muted">{outcome.mode} · ~{outcome.estimated_tokens} tokens</span>
          </div>
          <pre class="prompt">{outcome.optimized_prompt}</pre>
          <div class="actions">
            <button class="btn" onclick={onSend} disabled={llmBusy} aria-label="Send to LLM">
              {llmBusy ? "Sending…" : "Send to LLM"}
            </button>
            <button class="btn ghost" onclick={onCopy} aria-label="Copy prompt">Copy</button>
          </div>
        </div>

        {#if llmResponse}
          <div class="result-step">
            <span class="eyebrow">Response</span>
            <pre class="response">{llmResponse}</pre>
          </div>
        {/if}
      </section>
    {:else}
      <div class="record-placeholder">Press record or your hotkey to start.</div>
    {/if}
  </div>

  <div class="record-bar">
    <button
      class="record-btn {recState}"
      onclick={onToggle}
      disabled={recState === "decoding"}
      aria-label={stateLabel}
    >
      <span class="dot"></span>
    </button>
    <p class="record-state">{stateLabel}</p>
    <p class="record-hint">or press <kbd>{hotkey}</kbd> in any app</p>
    {#if recState === "recording"}
      <button class="text-btn" onclick={onCancel} aria-label="Cancel recording">Cancel</button>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: Update the CSS**

In `ui/src/app.css`, replace the `.record-hero` / `.record-hero.centered` rules (the block starting `/* ─── Record Button ─── */`, lines defining `.record-hero { ... }` and `.record-hero.centered { ... }`) with:

```css
/* ─── Record Button ─── */
.record-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.record-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
.record-placeholder {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--fg3);
  font-size: 13px;
}
.record-bar {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  padding-top: var(--space-4);
  margin-top: var(--space-3);
  border-top: 1px solid var(--border);
}
```

Note: keep the existing `.record-btn`, `.record-state`, `.record-hint` rules as-is — only the `.record-hero*` container rules are replaced.

- [ ] **Step 3: Let the Record view fill the content height**

In `ui/src/app.css`, the `.content > *` rule caps width and centers. Add a rule so the record view can stretch to full height (append right after the existing `.content > *` line):

```css
.content > .record-view { height: 100%; }
```

- [ ] **Step 4: Verify the build**

Run: `npm run build --prefix ui 2>&1 | tail -5`
Expected: `✓ built` with no errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/RecordingView.svelte ui/src/app.css
git commit -m "feat: pin record button to bottom of record view"
```

---

## Manual End-to-End Verification (after all tasks)

Build and run the app: `npm run build --prefix ui && cargo run -p pie-desktop` (or the packaged `.app`).

1. Record three short phrases (via hotkey and the in-app button). Open the **History** tab — all three appear, newest first.
2. **Copy** an entry, paste it into a text editor — the transcript lands.
3. Focus another app, use **Paste** on an entry — the transcript is typed into that app (PIE hides first).
4. **Delete** one entry — it disappears; the others remain.
5. **Clear all** — the list empties.
6. In **Settings → History**, set "Recordings to keep" to 1, record again — only the newest row remains in History.
7. **Search** — type a word from one transcript; the list filters to matching rows.
8. On the **Record** tab, confirm the record button sits at the bottom with result content scrolling above it, button position stable whether or not a result is shown.

## Spec Coverage Check

- SQLite store + migrations → Task 1
- Full-outcome schema + `language` → Task 1 (schema) + Task 4 (capture)
- Hard-cap retention (`history_limit`, default 10) → Task 1 (`prune`) + Task 2 (setting) + Task 4 (applied on capture)
- Capture at single chokepoint (hotkey + in-app) → Task 4
- Best-effort (never fails recording) → Task 4 (Step 5)
- Commands list/delete/clear/paste → Task 5; copy → Task 3
- `pie://history-changed` live refresh → Tasks 4/5 (emit) + Task 7 (listen)
- History tab UI with search + per-entry actions + clear all → Task 7
- Retention control in Settings → Task 6
- Record button pinned to bottom → Task 8
