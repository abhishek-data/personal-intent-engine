# Phase 4: Dual Hotkey System — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Two global hotkeys — one pastes the raw transcript, one pastes the PIE-optimized prompt — so the user gets either output without changing a setting.

**Architecture:** Each hotkey carries a paste mode (`"transcript"` or `"prompt"`). A new `AppState.pending_paste_mode` captures the mode when a recording starts (from the hotkey that fired, or from `settings.paste_output` for the UI record button), and the stop/paste path reads it — so the refine gate and paste selection use the mode that started this recording, not a global setting. `settings.paste_output` is retained as the default (UI record button + fallback); the single `hotkey` field is replaced by `hotkey_raw` + `hotkey_optimized`, with a migration for existing installs.

**Tech Stack:** Rust (edition 2021), tauri-plugin-global-shortcut (already a dep); Svelte 5.

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code; `unwrap_or_else(|e| e.into_inner())` on poisoned std-mutex is the allowed pattern in `src-tauri`. Doc comments on new public items.
- `cargo fmt` + clippy clean (ignore pre-existing `phonetic.rs:37`, `nspanel.rs:116`). Test output pristine.
- **Do not break the core recording flow.** A bad hotkey (invalid/empty) must not prevent the app from starting (existing behavior). Registration errors are logged, not fatal.
- Migration: existing installs with a legacy `hotkey` must not lose their binding.
- The hotkey/paste behavior CANNOT be unit-tested (global OS shortcuts + paste simulation). Verification for those tasks is a clean build plus an explicit MANUAL smoke test. Only the settings migration is unit-tested.
- Desktop crate `pie-desktop`.

---

### Task 1: Settings — `hotkey_raw` / `hotkey_optimized` + migration

**Files:**
- Modify: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `Settings.hotkey_raw: String` (default `"CmdOrCtrl+Shift+V"`), `Settings.hotkey_optimized: String` (default `"CmdOrCtrl+Shift+Space"`). The old `hotkey: String` field is REMOVED from the struct. `paste_output` stays. Migration lives in `Settings::load()`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `settings.rs`:

```rust
    #[test]
    fn dual_hotkey_defaults() {
        let s = Settings::default();
        assert_eq!(s.hotkey_raw, "CmdOrCtrl+Shift+V");
        assert_eq!(s.hotkey_optimized, "CmdOrCtrl+Shift+Space");
    }

    #[test]
    fn migrates_legacy_hotkey_by_paste_output() {
        // Legacy install: single hotkey + paste_output=prompt -> optimized.
        let legacy = r#"{"hotkey":"CmdOrCtrl+Alt+P","paste_output":"prompt"}"#;
        let migrated = Settings::from_json_migrating(legacy);
        assert_eq!(migrated.hotkey_optimized, "CmdOrCtrl+Alt+P");
        assert_eq!(migrated.hotkey_raw, "CmdOrCtrl+Shift+V"); // default keeps

        // Legacy install: single hotkey + paste_output=transcript -> raw.
        let legacy2 = r#"{"hotkey":"CmdOrCtrl+Alt+R","paste_output":"transcript"}"#;
        let m2 = Settings::from_json_migrating(legacy2);
        assert_eq!(m2.hotkey_raw, "CmdOrCtrl+Alt+R");
        assert_eq!(m2.hotkey_optimized, "CmdOrCtrl+Shift+Space"); // default keeps
    }

    #[test]
    fn new_install_no_legacy_key_uses_defaults() {
        let s = Settings::from_json_migrating(r#"{"mode":"balanced"}"#);
        assert_eq!(s.hotkey_raw, "CmdOrCtrl+Shift+V");
        assert_eq!(s.hotkey_optimized, "CmdOrCtrl+Shift+Space");
    }
```

- [ ] **Step 2: Run to verify fail** — `cargo test dual_hotkey_defaults migrates_legacy_hotkey_by_paste_output` → FAIL.

- [ ] **Step 3: Implement**

In `Settings`: remove `pub hotkey: String`; add:

```rust
    /// Global shortcut that pastes the raw transcript.
    pub hotkey_raw: String,
    /// Global shortcut that pastes the PIE-optimized prompt.
    pub hotkey_optimized: String,
```

In `Default`: remove the `hotkey:` line; add `hotkey_raw: "CmdOrCtrl+Shift+V".to_string(), hotkey_optimized: "CmdOrCtrl+Shift+Space".to_string(),`.

Add a migrating parse used by `load()` and the tests. Because `#[serde(default)]` fills missing new fields with the defaults, migration only needs to override them when a legacy `hotkey` is present:

```rust
impl Settings {
    /// Parse settings JSON, migrating a legacy single `hotkey` into the dual
    /// hotkeys based on the legacy `paste_output` (prompt -> optimized,
    /// otherwise raw). New installs (no legacy `hotkey`) keep the defaults.
    pub fn from_json_migrating(json: &str) -> Self {
        let mut s: Settings = serde_json::from_str(json).unwrap_or_default();
        // Look at the raw JSON for a legacy `hotkey` the struct no longer has.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
            let has_new = v.get("hotkey_raw").is_some() || v.get("hotkey_optimized").is_some();
            if let (false, Some(legacy)) = (has_new, v.get("hotkey").and_then(|h| h.as_str())) {
                if !legacy.trim().is_empty() {
                    let to_prompt = v.get("paste_output").and_then(|p| p.as_str()) == Some("prompt");
                    if to_prompt {
                        s.hotkey_optimized = legacy.to_string();
                    } else {
                        s.hotkey_raw = legacy.to_string();
                    }
                }
            }
        }
        s
    }
}
```

Change `Settings::load()` to use `from_json_migrating`:

```rust
    pub fn load() -> Self {
        let path = settings_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => Self::from_json_migrating(&json),
            Err(_) => Self::default(),
        }
    }
```

(The prior `unwrap_or_else` warn-on-parse-error is folded into `from_json_migrating`'s `unwrap_or_default`.)

- [ ] **Step 4: Run to verify pass** — `cargo test -p pie-desktop settings` (or bare test names) all pass; fix any other references to `settings.hotkey` that now fail to compile — but DO NOT wire main.rs yet beyond making it compile; if `main.rs` references `settings.hotkey`, this task may leave it broken. **Better:** this task ONLY touches settings.rs; if removing `hotkey` breaks `main.rs` compilation, that's expected and Task 2 fixes it. To keep the tree compiling between tasks, temporarily keep `main.rs` working by having Task 1 ALSO do the mechanical rename of `settings.hotkey` reads in main.rs to `settings.hotkey_optimized` as a stopgap (Task 2 replaces them properly). Note this in the report.

Run: `cargo build -p pie-desktop` — must compile.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-desktop
git add src-tauri/src/settings.rs src-tauri/src/main.rs
git commit -m "feat(settings): dual hotkeys (raw/optimized) + legacy migration"
```

---

### Task 2: `AppState.pending_paste_mode` + dual registration + mode threading

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `Settings.hotkey_raw`, `Settings.hotkey_optimized`, `Settings.paste_output` (Task 1).
- Produces: `AppState.pending_paste_mode: Mutex<String>`; `register_hotkeys(app, settings)`; `on_hotkey_with_mode(app, mode)`; `transcribe_and_process` reads the pending mode for the refine gate; `on_hotkey` paste selection reads the pending mode.

- [ ] **Step 1: `AppState` field + init**

Add `pending_paste_mode: Mutex<String>,` to `AppState`. Initialize in the `.setup` closure's `app.manage(AppState { .. })` to `Mutex::new(settings.paste_output.clone())` (capture before `settings` is moved, like the Phase-2 learner values).

- [ ] **Step 2: Set the pending mode when a recording starts**

In `do_start_recording`, after acquiring `settings`, set the default pending mode from `settings.paste_output` (UI record-button path):

```rust
    *state.pending_paste_mode.lock().unwrap_or_else(|e| e.into_inner()) = settings.paste_output.clone();
```

- [ ] **Step 3: `on_hotkey_with_mode` + registration**

Replace `register_hotkey` with `register_hotkeys` that registers BOTH shortcuts (each parsing independently; a failure on one is logged, not fatal, and doesn't block the other):

```rust
/// Register both global hotkeys. Each carries a paste mode. An invalid or
/// empty binding is logged and skipped — never fatal.
fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    register_one(app, &settings.hotkey_raw, "transcript");
    register_one(app, &settings.hotkey_optimized, "prompt");
    Ok(())
}

fn register_one(app: &AppHandle, hotkey: &str, mode: &'static str) {
    let trimmed = hotkey.trim();
    if trimmed.is_empty() {
        log::info!("Hotkey ({mode}) disabled");
        return;
    }
    let shortcut = match trimmed.parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(s) => s,
        Err(e) => { log::error!("Invalid hotkey '{hotkey}' ({mode}): {e}"); return; }
    };
    let res = app.global_shortcut().on_shortcut(shortcut, move |app, _fired, event| {
        if event.state == ShortcutState::Pressed {
            on_hotkey_with_mode(app, mode);
        }
    });
    if let Err(e) = res {
        log::error!("Failed to register hotkey '{hotkey}' ({mode}): {e}");
    } else {
        log::info!("Hotkey registered ({mode}): {hotkey}");
    }
}
```

Add `on_hotkey_with_mode`, adapting the existing `on_hotkey` body: when NOT recording, set the pending mode to `mode` before starting; when recording, stop and paste (the paste reads the pending mode). Keep the existing busy-flag guard:

```rust
fn on_hotkey_with_mode(app: &AppHandle, mode: &str) {
    let Some(state) = app.try_state::<AppState>() else { return; };
    let recording = state.recorder.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    if !recording {
        *state.pending_paste_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.to_string();
        if let Err(e) = do_start_recording(app) { log::error!("hotkey start: {e}"); }
    } else {
        // stop + paste (existing async stop path). Reuse the existing on_hotkey
        // stop logic verbatim, but read the pending paste mode instead of
        // settings.paste_output for the paste selection.
        // ... (see Step 4)
    }
}
```

Preserve the exact async stop/paste structure from the current `on_hotkey` (spawn, `do_stop_recording`, clipboard + `enigo` paste). Delete the old `on_hotkey` and `register_hotkey` once `on_hotkey_with_mode`/`register_hotkeys` replace them.

- [ ] **Step 4: Paste selection + refine gate read the pending mode**

- In the stop/paste code, replace `let text = if settings.paste_output == "prompt" { outcome.optimized_prompt } else { ... }` with a read of the pending mode: `let mode = state.pending_paste_mode.lock()…clone(); let text = if mode == "prompt" { outcome.optimized_prompt } else { outcome.transcript-or-corrected };` (match the exact current branches).
- In `transcribe_and_process`, change the Phase-5 refine gate from `if settings.paste_output == "prompt"` to read the pending mode: `let pmode = state.pending_paste_mode.lock().unwrap_or_else(|e| e.into_inner()).clone(); if pmode == "prompt" { … refine … }`.

- [ ] **Step 5: `update_settings` + `set_hotkey_active`**

- `update_settings`: detect a change to EITHER `hotkey_raw` or `hotkey_optimized` and call `register_hotkeys(&app, &settings)` (replacing the single-hotkey re-register). Keep the "register before persist" ordering so an invalid binding doesn't get saved as broken (though registration is now non-fatal, still re-register on change).
- `set_hotkey_active`: on `active=true`, re-register BOTH via `register_hotkeys`; on `active=false`, `gs.unregister_all()`. (The recorder component suspends during capture.)

- [ ] **Step 6: Build + verify**

Run: `cargo build -p pie-desktop` clean; `cargo test -p pie-engine --lib` unaffected (still pass); clippy no new warnings.

- [ ] **Step 7: MANUAL smoke test (required — not automatable)**

Run the app. Bind distinct raw/optimized hotkeys. Confirm: raw hotkey → records → pastes the raw transcript; optimized hotkey → records → pastes the optimized prompt; both show the overlay; busy-flag prevents double-trigger; UI record button still works (uses `paste_output` default). Confirm a legacy install (old `settings.json` with `hotkey`) migrates and still fires.

- [ ] **Step 8: Commit**

```bash
cargo fmt && cargo clippy -p pie-desktop
git add src-tauri/src/main.rs
git commit -m "feat(app): dual global hotkeys with per-hotkey paste mode"
```

---

### Task 3: UI — two hotkey recorders; keep paste_output as default

**Files:**
- Modify: `ui/src/lib/HotkeyRecorder.svelte`
- Modify: `ui/src/App.svelte`
- Modify: `ui/src/lib/OutputSettings.svelte` (relabel paste_output)

**Interfaces:** UI only.

- [ ] **Step 1: Parameterize `HotkeyRecorder`**

Read `ui/src/lib/HotkeyRecorder.svelte` fully. It currently edits `settings.hotkey`. Change it to accept a `field` prop (the settings key it edits, e.g. `"hotkey_raw"`) and a `label` prop, reading/writing `settings[field]`. The `set_hotkey_active` suspend/restore calls stay the same (they suspend ALL hotkeys during capture, which is correct). Keep `{ settings, onSave, onError }` plus the new `field`/`label`.

- [ ] **Step 2: Two recorders in the settings UI**

Wherever the single `HotkeyRecorder` is rendered (find it in `App.svelte` / the settings layout), render two:

```svelte
<HotkeyRecorder {settings} onSave={save} onError={...} field="hotkey_raw" label="Raw paste hotkey" />
<HotkeyRecorder {settings} onSave={save} onError={...} field="hotkey_optimized" label="Optimized paste hotkey" />
```

Update the `RecordingView` hotkey hint (`hotkey={settings.hotkey}` at App.svelte ~209) to show the optimized hotkey (`settings.hotkey_optimized`) or both.

- [ ] **Step 3: `OutputSettings` — relabel paste_output**

The paste_output segmented control now controls the DEFAULT output (UI record button / fallback), not the only output. Update its label/caption to say so (e.g. "Record-button output" with a caption noting the two hotkeys have fixed outputs). Do not remove it.

- [ ] **Step 4: Build** — `npm run build` from `ui/` clean.

- [ ] **Step 5: Commit**

```bash
git add ui/src/lib/HotkeyRecorder.svelte ui/src/App.svelte ui/src/lib/OutputSettings.svelte
git commit -m "feat(ui): dual hotkey recorders + record-button default output"
```

---

## Acceptance (Phase 4 spec)

- [ ] Two separate global hotkeys registered — Task 2.
- [ ] Raw hotkey: record → transcribe → correct → paste raw text — Tasks 2 (manual verify).
- [ ] Optimized hotkey: record → transcribe → correct → intent → optimize → paste prompt — Tasks 2 (manual verify).
- [ ] Both show the overlay; busy-flag prevents double-trigger — Task 2.
- [ ] Settings UI has two hotkey fields — Task 3.
- [ ] Legacy single-hotkey installs migrate without losing their binding — Task 1 (unit-tested).

## Deviation note
The spec said `paste_output` is "subsumed by hotkey choice." This plan RETAINS `paste_output` as the default output for the UI record button (which has no hotkey mode) and as the runtime default; the two hotkeys override it per-press via `pending_paste_mode`. Fully removing `paste_output` would break the UI-record-button path. Flagged for the reviewer/user.
