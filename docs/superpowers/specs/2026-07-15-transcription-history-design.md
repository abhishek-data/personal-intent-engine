# Transcription History — Design Spec

Date: 2026-07-15
Status: Approved (ready for implementation planning)

## Goal

Persist every recording locally and add a searchable **History** tab to the
desktop app. Store the full pipeline outcome (not just the transcript) so the
data is useful later for analyzing/training on user behavior. Bundled together
with a small Record-view layout change (record button pinned to the bottom,
OpenSuperWhisper-style).

## Scope

In scope:
- Local SQLite store of recording outcomes.
- Automatic capture on every recording (hotkey and in-app Record button).
- History tab: search, list newest-first, per-entry Copy / Paste / Delete,
  Clear all.
- A hard retention cap setting (`history_limit`, default 10).
- Record-view layout: record button pinned at the bottom, content above.

Out of scope (documented as easy future migrations):
- Storing the LLM response alongside a row (schema can add an `llm_response`
  column later via the migration runner).
- Full-text search (FTS5) — start with SQL `LIKE`; bundled SQLite includes FTS5
  so this is a later upgrade.
- History on/off privacy toggle.
- Re-run-through-pipeline action from a history entry.

## Storage

- **Engine:** SQLite via `rusqlite` with the `bundled` feature. Compiles the
  SQLite amalgamation into the binary (~1–1.5 MB, no system dependency; also
  keeps future Windows/Linux builds clean). In-process, microsecond
  reads/writes — no perceptible performance or memory burden.
- **Location:** `~/<config>/pie/history.db` (same `dirs::config_dir()/pie`
  directory as `settings.json` and `memory.json`).
- **Migrations:** a tiny runner keyed on `PRAGMA user_version`. Version 1
  creates the `history` table. Adding columns later bumps the version and runs
  the next migration, so existing DBs upgrade in place without breaking.

### Schema (migration v1)

```sql
CREATE TABLE IF NOT EXISTS history (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at        INTEGER NOT NULL,   -- unix seconds
  transcript        TEXT    NOT NULL,
  objective         TEXT,
  conversation_type TEXT,
  confidence        TEXT,
  optimized_prompt  TEXT,
  estimated_tokens  INTEGER,
  mode              TEXT,
  language          TEXT                -- settings.language at capture time
);
CREATE INDEX IF NOT EXISTS idx_history_created_at ON history(created_at DESC);
```

Rows are listed newest-first by `id` (monotonic, so equivalent to insertion order).

## Retention

- New setting `history_limit: usize`, **default 10**.
- **Hard cap:** after each insert, prune anything beyond the newest `limit`:
  ```sql
  DELETE FROM history
  WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT ?1);
  ```
- Lowering the setting prunes on the next insert. (Implementation may also prune
  immediately when settings are saved; either is acceptable — insert-time prune
  is the guarantee.)
- Consequence, accepted by design: later behavior analysis only ever sees the
  most recent `history_limit` rows. A user who wants more raises the cap.

## Core library

New module `src/history/mod.rs` in the core lib (keeps it unit-testable and
consistent with the library-first structure).

```rust
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: i64,          // unix seconds
    pub transcript: String,
    pub objective: Option<String>,
    pub conversation_type: Option<String>,
    pub confidence: Option<String>,
    pub optimized_prompt: Option<String>,
    pub estimated_tokens: Option<i64>,
    pub mode: Option<String>,
    pub language: Option<String>,
}

pub struct NewEntry { /* same fields minus id/created_at */ }

pub struct HistoryStore { conn: rusqlite::Connection }

impl HistoryStore {
    pub fn open(path: &Path) -> anyhow::Result<Self>;   // opens + runs migrations
    pub fn open_in_memory() -> anyhow::Result<Self>;    // for tests
    pub fn add(&self, entry: NewEntry, limit: usize) -> anyhow::Result<i64>; // insert + prune, returns id
    pub fn list(&self, query: Option<&str>, limit: usize) -> anyhow::Result<Vec<HistoryEntry>>;
    pub fn delete(&self, id: i64) -> anyhow::Result<()>;
    pub fn clear(&self) -> anyhow::Result<()>;
}
```

- `list` with `query = Some(q)` filters `WHERE transcript LIKE '%'||?||'%'`
  (server-side), ordered `id DESC`, capped by `limit` (a generous display limit,
  e.g. the retention cap).
- All methods are synchronous; the Tauri layer wraps DB access in a `Mutex` and
  calls are cheap enough to run inline.

## Capture point

In `transcribe_and_process` ([src-tauri/src/main.rs:137](../../../src-tauri/src/main.rs)),
immediately after a non-empty transcript + successful `engine.process(...)`,
build a `NewEntry` from the `Outcome` fields plus `settings.language` and call
`history.add(entry, settings.history_limit)`. This single chokepoint covers both
the global hotkey flow and the in-app Record button.

A failure to write history must **not** fail the recording — log a warning and
continue (history is best-effort, transcription/paste is the primary path).

## App state & settings

- `AppState` gains `history: Mutex<HistoryStore>`, opened at startup (same place
  settings are loaded). If the DB fails to open, log and fall back to an
  in-memory store so the app still runs.
- `Settings` (src-tauri/src/settings.rs) gains `history_limit: usize` with
  `#[serde(default)]` compatibility and `Default = 10`. The `Default` impl and
  the frontend `settings` object in `App.svelte` are updated to include it.

## Tauri commands

- `list_history(query: Option<String>) -> Vec<HistoryEntry>`
- `delete_history_entry(id: i64) -> Result<(), String>`
- `clear_history() -> Result<(), String>`
- `paste_history_entry(id: i64) -> Result<(), String>` — looks up the row, hides
  the main window (so focus returns to the previously active app), briefly
  waits, then calls the existing `paste::paste_text`.
- Copy is handled by the existing `copy_to_clipboard` command (frontend passes
  the transcript text).

After any mutation (capture, delete, clear) the backend emits
`pie://history-changed` so an open History tab refreshes live.

## UI

New **History** tab, added as the 4th entry in `TABS` in `App.svelte`, routed to
a new `ui/src/lib/HistoryView.svelte`.

Layout (matches existing dark/indigo design system, no new libraries):
- A search input at the top; typing re-queries `list_history(query)` (debounced).
- A scrollable list, newest-first. Each row shows the transcript (truncated to a
  couple of lines) and a relative timestamp (e.g. "2m ago"), with Copy / Paste /
  Delete controls.
- A "Clear all" action at the bottom (with a confirm step).
- Empty state: a short invitation to record.

Data flow: on mount and on `pie://history-changed`, call `list_history`. Actions
call the matching commands, then refresh.

### Record-view layout change

Restructure `RecordingView.svelte` so the record button is **pinned at the
bottom** of the Record view (OpenSuperWhisper-style), with the transcript/result
content filling and scrolling in the space above it. The button stays in a fixed
position whether or not a result is present. State label + hint stay with the
button in the bottom bar; the Cancel control (while recording) stays adjacent to
the button. This is a CSS/markup change only — no logic changes to the recording
flow.

## Error handling

- History write failure: log warning, do not interrupt recording/paste.
- DB open failure at startup: log, fall back to `:memory:` store.
- `paste_history_entry` for a missing id: return a descriptive error string.
- `delete`/`clear` failures: surface as error strings to the UI error banner.

## Testing

Unit tests (`src/history/mod.rs`, against `open_in_memory`):
- `add` inserts and returns an id; `list` returns newest-first.
- Hard cap: adding beyond `limit` deletes the oldest rows; count stays == limit.
- Lowering the effective limit on a later `add` prunes to the new limit.
- `list(query)` filters by substring, case-insensitively where practical.
- `delete(id)` removes one row; `clear()` empties the table.
- Migration idempotency: opening an existing DB twice does not error or
  duplicate schema.

Manual E2E:
- Record 3 times; confirm all appear newest-first in the History tab.
- Copy → paste elsewhere; Paste-into-active-app lands text in the prior app;
  Delete removes a single entry; Clear all empties the list.
- Set `history_limit` to 1, record again, confirm only the newest row remains.
- Confirm the Record button is bottom-pinned with results scrolling above.

## Rollout / migration notes

- First launch with the new build creates `history.db` automatically.
- Existing users are unaffected; no data migration needed (new store).
- Adding `history_limit` to settings is backward-compatible via `#[serde(default)]`.
