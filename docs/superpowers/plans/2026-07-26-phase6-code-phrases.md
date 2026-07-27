# Phase 6: Code-Aware Post-Processing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Translate spoken code patterns into syntax ("console dot log" → "console.log(", "triple equals" → "===") — but ONLY when the user has turned on an explicit code mode, so ordinary dictation is never corrupted.

**Architecture:** A pure `src/corrector/code_phrases.rs` module holds a phrase→syntax map (loadable from JSON) and `apply_code_phrases(text)` that does longest-phrase-first, replace-ALL-occurrences, case-insensitive substitution (fixing the original design's single-`.find()` first-match-only bug). The engine holds a `code_mode` flag (set from settings); in `process()`, when code mode is on, it applies code phrases to the corrected transcript AFTER pronunciation correction and BEFORE intent extraction. Off by default; ordinary speech is untouched.

**Tech Stack:** Rust (edition 2021), serde_json (for the optional JSON map); Tauri v2; Svelte 5.

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code (tests may). Doc comments on public items. `cargo fmt` + clippy clean (ignore pre-existing `phonetic.rs:37`, `nspanel.rs:116`). Test output pristine.
- **Code mode is OFF by default** (`settings.code_mode` defaults `false`). When off, transcripts are byte-identical to today — code phrases NEVER run.
- Longest-phrase-first ordering; replace ALL occurrences (not just the first); case-insensitive matching.
- Code phrases run AFTER pronunciation correction, BEFORE intent extraction.
- Desktop crate `pie-desktop`; library `pie-engine`.

---

### Task 1: `code_phrases.rs` — the map + `apply_code_phrases`

**Files:**
- Create: `src/corrector/code_phrases.rs`
- Modify: `src/corrector/mod.rs` (`pub mod code_phrases;`)

**Interfaces:**
- Produces:
  - `pub fn builtin_map() -> Vec<(String, String)>` — the built-in spoken→syntax pairs.
  - `pub fn apply_code_phrases(text: &str) -> String` — using `builtin_map()`.
  - `pub fn apply_with_map(text: &str, map: &[(String, String)]) -> String` — the core (testable with a custom map). Longest-phrase-first (by phrase char length, descending), case-insensitive, replace ALL occurrences.
  - `pub fn map_from_json(json: &str) -> anyhow::Result<Vec<(String, String)>>` — parse a `{"spoken":"syntax", ...}` object (or `[["spoken","syntax"], ...]` array) into pairs, for a future user-supplied map.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_all_occurrences_longest_first() {
        let map = vec![
            ("console dot log".to_string(), "console.log(".to_string()),
            ("dot log".to_string(), ".log(".to_string()),
            ("triple equals".to_string(), "===".to_string()),
        ];
        // "console dot log" must win over "dot log" (longest first),
        // and BOTH "triple equals" occurrences must be replaced.
        let out = apply_with_map("console dot log then triple equals and triple equals", &map);
        assert_eq!(out, "console.log( then === and ===");
    }

    #[test]
    fn case_insensitive_match() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        assert_eq!(apply_with_map("Triple Equals", &map), "===");
    }

    #[test]
    fn no_match_returns_input_unchanged() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        assert_eq!(apply_with_map("hello world", &map), "hello world");
    }

    #[test]
    fn builtin_map_translates_common_patterns() {
        let out = apply_code_phrases("console dot log hello");
        assert!(out.starts_with("console.log("), "got: {out}");
    }

    #[test]
    fn map_from_json_object_and_array() {
        let obj = map_from_json(r#"{"dot map":".map("}"#).unwrap();
        assert_eq!(obj, vec![("dot map".to_string(), ".map(".to_string())]);
        let arr = map_from_json(r#"[["dot map",".map("]]"#).unwrap();
        assert_eq!(arr, vec![("dot map".to_string(), ".map(".to_string())]);
    }
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p pie-engine --lib corrector::code_phrases` → FAIL.

- [ ] **Step 3: Implement**

Create `src/corrector/code_phrases.rs`:

```rust
//! Code-aware post-processing: translate spoken code patterns into syntax.
//! Runs ONLY in code mode (opt-in), after pronunciation correction and before
//! intent extraction, so ordinary dictation is never affected.

/// Built-in spoken->syntax pairs. Ordering here doesn't matter — application
/// sorts by phrase length (longest first) so multi-word phrases win.
pub fn builtin_map() -> Vec<(String, String)> {
    [
        ("console dot log", "console.log("),
        ("dot log", ".log("),
        ("dot map", ".map("),
        ("dot filter", ".filter("),
        ("dot for each", ".forEach("),
        ("dot find", ".find("),
        ("dot push", ".push("),
        ("arrow function", "() => "),
        ("fat arrow", "() => "),
        ("triple equals", "==="),
        ("not strictly equal", "!=="),
        ("double equals", "=="),
        ("not equal", "!="),
        ("open brace", "{"),
        ("close brace", "}"),
        ("open bracket", "["),
        ("close bracket", "]"),
        ("open paren", "("),
        ("close paren", ")"),
        ("semi colon", ";"),
        ("single quote", "'"),
        ("double quote", "\""),
        ("back tick", "`"),
        ("hash tag", "#"),
        ("async function", "async function"),
        ("export default", "export default"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

/// Apply the built-in code-phrase map.
#[must_use]
pub fn apply_code_phrases(text: &str) -> String {
    apply_with_map(text, &builtin_map())
}

/// Apply `map` to `text`: longest phrase first, case-insensitive, ALL
/// occurrences. Matching is done on a lowercased copy while slicing the
/// original so replacement is stable.
#[must_use]
pub fn apply_with_map(text: &str, map: &[(String, String)]) -> String {
    let mut pairs: Vec<&(String, String)> = map.iter().collect();
    pairs.sort_by_key(|(spoken, _)| std::cmp::Reverse(spoken.chars().count()));

    let mut result = text.to_string();
    for (spoken, syntax) in pairs {
        if spoken.is_empty() {
            continue;
        }
        result = replace_all_ci(&result, spoken, syntax);
    }
    result
}

/// Case-insensitive replace-all of `needle` with `replacement` in `haystack`.
fn replace_all_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    let hay_lower = haystack.to_lowercase();
    let need_lower = needle.to_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0;
    while i < haystack.len() {
        if hay_lower[i..].starts_with(&need_lower) && haystack.is_char_boundary(i) {
            out.push_str(replacement);
            i += need_lower.len();
        } else {
            // Advance one char (respecting UTF-8 boundaries).
            let ch = haystack[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Parse a user-supplied map from JSON: either an object
/// `{"spoken":"syntax"}` or an array of pairs `[["spoken","syntax"]]`.
pub fn map_from_json(json: &str) -> anyhow::Result<Vec<(String, String)>> {
    use serde_json::Value;
    let v: Value = serde_json::from_str(json)?;
    match v {
        Value::Object(o) => Ok(o
            .into_iter()
            .filter_map(|(k, val)| val.as_str().map(|s| (k, s.to_string())))
            .collect()),
        Value::Array(a) => Ok(a
            .into_iter()
            .filter_map(|pair| {
                let arr = pair.as_array()?;
                let spoken = arr.first()?.as_str()?.to_string();
                let syntax = arr.get(1)?.as_str()?.to_string();
                Some((spoken, syntax))
            })
            .collect()),
        _ => anyhow::bail!("code-phrase map must be a JSON object or array of pairs"),
    }
}
```

> NOTE on `replace_all_ci`: the `hay_lower[i..]` byte-index slice is valid because `i` only ever lands on a char boundary of `haystack`, and for ASCII-heavy code phrases the lowercase map is 1:1 in byte length. If a non-ASCII phrase is ever added whose lowercase changes byte length, this indexing could misalign — the built-in map is ASCII-only, so this is safe today; the doc comment on `map_from_json` should note user maps should stay ASCII. If the implementer prefers, a simpler correct approach is to lowercase-compare per candidate window without slicing `hay_lower` — use whichever is cleanest and provably correct, and keep the tests.

Add `pub mod code_phrases;` to `src/corrector/mod.rs`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p pie-engine --lib corrector::code_phrases` then `cargo test -p pie-engine --lib` all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/code_phrases.rs src/corrector/mod.rs
git commit -m "feat(corrector): code-phrase map + apply (longest-first, replace-all)"
```

---

### Task 2: Engine — `code_mode` flag + apply in `process()`

**Files:**
- Modify: `src/pipeline/engine.rs`

**Interfaces:**
- Consumes: `code_phrases::apply_code_phrases` (Task 1).
- Produces:
  - `PieEngine.code_mode: bool` field (default `false`), initialized in `new()`, `new_ephemeral()`, `with_config()` delegation.
  - `pub fn set_code_mode(&mut self, on: bool)`.
  - In `process()`: after `self.corrector.correct(...)`, when `self.code_mode` is true, apply `code_phrases::apply_code_phrases` to the corrected text; use the result as the input to intent extraction AND as `corrected_transcript`. When off, behavior is unchanged.

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn code_mode_translates_spoken_code() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-cm-user-{uid}.json"));
        let lpath = dir.join(format!("pie-cm-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        engine.set_code_mode(true);
        let res = engine.process("console dot log hello", "compact").await.unwrap();
        assert!(
            res.corrected_transcript.contains("console.log("),
            "got: {}",
            res.corrected_transcript
        );
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }

    #[tokio::test]
    async fn code_mode_off_leaves_transcript_untouched() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-cm2-user-{uid}.json"));
        let lpath = dir.join(format!("pie-cm2-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        // default: code_mode off
        let res = engine.process("open the bracket please", "compact").await.unwrap();
        assert!(!res.corrected_transcript.contains('['), "off mode must not translate");
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p pie-engine --lib pipeline::engine` → FAIL (field/method missing).

- [ ] **Step 3: Implement**

1. Add `code_mode: bool` to `PieEngine`; init `false` in every constructor.
2. Add:

```rust
    /// Enable/disable code-aware post-processing (spoken code -> syntax).
    pub fn set_code_mode(&mut self, on: bool) {
        self.code_mode = on;
    }
```

3. In `process()`, find the block that computes the corrected text (currently `let correction = self.corrector.correct(input, &allowed); let input = correction.text.as_str();`). Replace with:

```rust
        let correction = self.corrector.correct(input, &allowed);
        let corrected_text = if self.code_mode {
            crate::corrector::code_phrases::apply_code_phrases(&correction.text)
        } else {
            correction.text.clone()
        };
        let input = corrected_text.as_str();
```

Then ensure the `PieResult { corrected_transcript, .. }` uses `corrected_text.clone()` (not `correction.text.clone()`), and reinforcement still iterates `correction.applied` (unchanged — code phrases aren't pronunciation fixes and aren't tracked as AppliedFix). The `applied` field stays `correction.applied`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p pie-engine --lib` all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/pipeline/engine.rs
git commit -m "feat(engine): apply code phrases in process() when code mode on"
```

---

### Task 3: Settings + desktop/CLI wiring + UI toggle

**Files:**
- Modify: `src-tauri/src/settings.rs` (add `code_mode: bool`)
- Modify: `src-tauri/src/main.rs` (set on engine at startup + on settings change)
- Modify: `src/main.rs` (CLI `--code-mode` flag)
- Modify: `ui/src/lib/VocabularySettings.svelte` (or another settings pane) — code-mode toggle

**Interfaces:**
- Consumes: `engine.set_code_mode` (Task 2).
- Produces: `Settings.code_mode: bool` (default false); engine gets it at startup and on `update_settings`; CLI `--code-mode`; a UI checkbox.

- [ ] **Step 1: Settings field + test**

Add `pub code_mode: bool` next to `background_mining` (default `false`). Extend a settings test to assert the default is false and it roundtrips.

- [ ] **Step 2: Desktop wiring**

- Startup (`.setup` closure): after the engine is built and managed, set the flag — simplest is to build the engine already knowing it. Since the engine is created via `PieEngine::with_config`, add a line right after `app.manage(AppState {..})` (or before, on the local `engine`): `engine.set_code_mode(settings.code_mode);` BEFORE it's moved into `AppState`. (Place it right after the engine is built at the top of the closure, using the loaded `settings`.)
- `update_settings`: detect a `code_mode` change and apply it: after saving, `let mut engine = state.engine.lock().await; engine.set_code_mode(settings.code_mode);` (fold into the existing `llm_changed` async block, or add a parallel one — keep the std settings guard dropped before the await, per the existing pattern).

- [ ] **Step 3: CLI flag**

In `src/main.rs` `Args`, add `#[arg(long)] code_mode: bool`. After building the engine, `engine.set_code_mode(args.code_mode);` before `process`.

- [ ] **Step 4: UI toggle**

In `ui/src/lib/VocabularySettings.svelte` (it already hosts correction-related toggles), add a **Code mode** checkbox bound to `settings.code_mode`, `onchange={onSave}`, with a caption: "Translate spoken code (“console dot log” → console.log() ). Only turn on while dictating code." Reuse existing `toggle-row`/`toggle-label`/`toggle-caption` classes.

- [ ] **Step 5: Build + verify**

`cargo build -p pie-desktop` clean; `cargo build` (CLI) clean; `cargo test -p pie-engine --lib` pass; `npm run build` clean; clippy no new warnings.

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine && cargo clippy -p pie-desktop
git add src-tauri/src/settings.rs src-tauri/src/main.rs src/main.rs ui/src/lib/VocabularySettings.svelte
git commit -m "feat(app): code-mode setting + wiring + UI toggle"
```

---

## Acceptance (Phase 6 spec)

- [ ] With code mode OFF (default), transcripts are never altered by code phrases — Tasks 2/3.
- [ ] With code mode ON: "console dot log hello" → "console.log(hello" — Tasks 1/2.
- [ ] Longest-phrase-first prevents partial replacements; ALL occurrences fixed — Task 1.
- [ ] Runs after pronunciation dict, before intent extraction — Task 2.
- [ ] Map is loadable/extensible from JSON (`map_from_json`) — Task 1.
