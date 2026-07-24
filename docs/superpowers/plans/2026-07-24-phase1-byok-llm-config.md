# Phase 1: BYOK LLM Config — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user configure LLM API URL + key + model from the settings UI (BYOK), with env vars as the fallback, and a Test Connection button.

**Architecture:** Introduce an `LlmConfig` value object built from `Settings`. `LlmRouter` gains `from_config()` which builds an `OpenAiClient` from the config when a URL is set, else falls back to `OpenAiClient::from_env()`. The engine can rebuild its router when settings change, and a `test_llm_connection` command verifies typed-but-unsaved credentials with a 1-token round-trip.

**Tech Stack:** Rust (edition 2021), tokio, reqwest, serde; Tauri v2 commands; Svelte 5 (runes) frontend.

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code — use `?` or `.expect("reason")`. (`unwrap_or_else(|e| e.into_inner())` on poisoned mutex locks in `src-tauri` is the established pattern and is allowed.)
- Doc comments (`/// …`) on all public items. Run `cargo fmt` and `cargo clippy` before each commit.
- Backward compatibility: `pie --provider openai` (env-var path) MUST keep working. Existing installs with no `llm_api_url` set MUST behave exactly as before.
- API key is a secret: password input in the UI; never logged, never rendered in plaintext.
- Settings persist as JSON at `~/.config/pie/settings.json` via `#[serde(default)]`, so added fields must have `Default` values (empty string).
- macOS signing cert must not be touched (no build/signing changes in this phase).

---

### Task 1: `LlmConfig` + `LlmRouter::from_config`

**Files:**
- Modify: `src/llm/router.rs`
- Modify: `src/llm/mod.rs` (export `LlmConfig`)

**Interfaces:**
- Produces:
  - `pub struct LlmConfig { pub api_url: String, pub api_key: String, pub model: String }`
  - `impl LlmRouter { pub fn from_config(config: &LlmConfig) -> Self }` — builds an `OpenAiClient` from `config` when `api_url` is non-empty (trimmed), else falls back to `OpenAiClient::from_env()`.
  - Existing `LlmRouter::new()`, `send()`, `is_available()` unchanged.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/llm/router.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_with_url_is_available() {
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        let router = LlmRouter::from_config(&cfg);
        assert!(router.is_available("openai"));
    }

    #[test]
    fn from_config_empty_url_falls_back_to_env() {
        // No OPENAI_API_KEY in the test env => env fallback yields no client.
        std::env::remove_var("OPENAI_API_KEY");
        let cfg = LlmConfig {
            api_url: String::new(),
            api_key: String::new(),
            model: String::new(),
        };
        let router = LlmRouter::from_config(&cfg);
        assert!(!router.is_available("openai"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib llm::router`
Expected: FAIL — `cannot find struct LlmConfig` / `no function from_config`.

- [ ] **Step 3: Write minimal implementation**

At the top of `src/llm/router.rs`, keep `use super::openai::OpenAiClient;` and add the config struct plus the constructor. Insert `LlmConfig` above the `LlmRouter` struct, and add `from_config` inside `impl LlmRouter` (next to `new`):

```rust
/// User-provided LLM connection settings (Bring Your Own Key).
///
/// When `api_url` is empty the router falls back to environment variables
/// (`OPENAI_API_KEY` / `OPENAI_BASE_URL`), preserving the CLI/env path.
pub struct LlmConfig {
    /// OpenAI-compatible base URL, e.g. `https://api.openai.com/v1`.
    pub api_url: String,
    /// Bearer token; may be empty for local servers that need no key.
    pub api_key: String,
    /// Default model name; empty means "use the provider default".
    pub model: String,
}
```

```rust
    /// Build a router from user settings (BYOK). When `config.api_url` is
    /// empty, falls back to environment variables so the CLI/env path keeps
    /// working.
    #[must_use]
    pub fn from_config(config: &LlmConfig) -> Self {
        if config.api_url.trim().is_empty() {
            Self::new()
        } else {
            Self {
                client: Some(OpenAiClient::new(&config.api_url, &config.api_key)),
            }
        }
    }
```

In `src/llm/mod.rs`, extend the re-export:

```rust
pub use router::{LlmConfig, LlmRouter};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib llm::router`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/llm/router.rs src/llm/mod.rs
git commit -m "feat(llm): add LlmConfig + LlmRouter::from_config for BYOK"
```

---

### Task 2: Settings fields for BYOK

**Files:**
- Modify: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `Settings.llm_api_url: String` and `Settings.llm_api_key: String` (both default `String::new()`); existing `llm_model: String` reused. `#[serde(default)]` on the struct means older `settings.json` files without these keys load fine.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/settings.rs`:

```rust
    #[test]
    fn byok_fields_default_empty_and_roundtrip() {
        let loaded: Settings = serde_json::from_str(r#"{"mode":"compact"}"#).unwrap();
        assert_eq!(loaded.llm_api_url, "");
        assert_eq!(loaded.llm_api_key, "");

        let s = Settings {
            llm_api_url: "https://api.openai.com/v1".into(),
            llm_api_key: "sk-abc".into(),
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.llm_api_url, "https://api.openai.com/v1");
        assert_eq!(back.llm_api_key, "sk-abc");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie --bin pie settings::tests::byok_fields_default_empty_and_roundtrip` — if the binary/package name differs, use `cargo test byok_fields_default_empty_and_roundtrip`.
Expected: FAIL — `no field llm_api_url on type Settings`.

- [ ] **Step 3: Write minimal implementation**

In the `Settings` struct in `src-tauri/src/settings.rs`, add the two fields next to `llm_model`:

```rust
    /// OpenAI-compatible base URL for BYOK (empty = fall back to env vars).
    pub llm_api_url: String,
    /// API key / bearer token for BYOK (empty = none / local server).
    pub llm_api_key: String,
```

In the `Default for Settings` impl, add:

```rust
            llm_api_url: String::new(),
            llm_api_key: String::new(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test byok_fields_default_empty_and_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy
git add src-tauri/src/settings.rs
git commit -m "feat(settings): add llm_api_url + llm_api_key BYOK fields"
```

---

### Task 3: Engine builds/rebuilds its router from `LlmConfig`

**Files:**
- Modify: `src/pipeline/engine.rs`

**Interfaces:**
- Consumes: `LlmConfig`, `LlmRouter::from_config` (Task 1).
- Produces:
  - `impl PieEngine { pub async fn with_config(config: &crate::llm::LlmConfig) -> anyhow::Result<Self> }` — same as `new()` but builds the router via `from_config`.
  - `impl PieEngine { pub fn set_llm_config(&mut self, config: &crate::llm::LlmConfig) }` — rebuilds `self.llm` in place (used when settings change at runtime).
  - Existing `new()`, `new_ephemeral()`, `process()`, `send_to_llm()` unchanged.

- [ ] **Step 1: Write the failing test**

Add a test module at the bottom of `src/pipeline/engine.rs` (there is none yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmConfig;

    #[tokio::test]
    async fn with_config_uses_configured_client() {
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".into(),
            api_key: "sk-x".into(),
            model: "gpt-4o-mini".into(),
        };
        let engine = PieEngine::with_config(&cfg).await.unwrap();
        assert!(engine.llm.is_available("openai"));
    }

    #[tokio::test]
    async fn set_llm_config_rebuilds_router() {
        std::env::remove_var("OPENAI_API_KEY");
        let mut engine = PieEngine::new().await.unwrap();
        assert!(!engine.llm.is_available("openai"));
        engine.set_llm_config(&LlmConfig {
            api_url: "https://api.example.com/v1".into(),
            api_key: "sk-x".into(),
            model: String::new(),
        });
        assert!(engine.llm.is_available("openai"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib pipeline::engine`
Expected: FAIL — `no function with_config` / `no method set_llm_config`. (The `engine.llm` field is private but the test is in the same module, so field access compiles once the methods exist.)

- [ ] **Step 3: Write minimal implementation**

In `src/pipeline/engine.rs`, add `use crate::llm::LlmConfig;` is not needed at top (tests import it); add these methods inside `impl PieEngine`, right after `new()`:

```rust
    /// Initialize the engine with a BYOK LLM config (falls back to env vars
    /// when the config URL is empty).
    pub async fn with_config(config: &crate::llm::LlmConfig) -> anyhow::Result<Self> {
        let mut engine = Self::new().await?;
        engine.set_llm_config(config);
        Ok(engine)
    }

    /// Rebuild the LLM router from a new config (e.g. after the user edits
    /// LLM settings). Does not touch memory, STT, or the corrector.
    pub fn set_llm_config(&mut self, config: &crate::llm::LlmConfig) {
        self.llm = crate::llm::LlmRouter::from_config(config);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib pipeline::engine`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/pipeline/engine.rs
git commit -m "feat(engine): build/rebuild LLM router from LlmConfig"
```

---

### Task 4: Wire config at startup, rebuild on save, add `test_llm_connection`

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `Settings.llm_api_url/llm_api_key/llm_model` (Task 2), `PieEngine::with_config` + `set_llm_config` (Task 3), `LlmConfig` (Task 1), existing `OpenAiClient` for the test round-trip.
- Produces: `#[tauri::command] async fn test_llm_connection(url: String, key: String, model: String) -> Result<String, String>` returning `Ok("connected")` on success or `Err(message)` on failure. Registered in `generate_handler!`.

- [ ] **Step 1: Add a helper to build `LlmConfig` from `Settings`**

Near `model_opt` (around `src-tauri/src/main.rs:750`), add:

```rust
/// Build the engine's LLM connection config from settings (BYOK).
fn llm_config(s: &Settings) -> pie_engine::llm::LlmConfig {
    pie_engine::llm::LlmConfig {
        api_url: s.llm_api_url.clone(),
        api_key: s.llm_api_key.clone(),
        model: s.llm_model.clone(),
    }
}
```

Ensure `pie_engine::llm::LlmConfig` is reachable — the crate re-exports `llm` as a public module (`pie_engine::llm`). If a top-level re-export is preferred, add `pub use llm::LlmConfig;` in `src/lib.rs` alongside existing exports and use `pie_engine::LlmConfig` instead. Pick whichever matches the existing import style in `main.rs` and use it consistently.

- [ ] **Step 2: Build the engine from config at startup**

In `main()` (around `src-tauri/src/main.rs:844`), replace:

```rust
            let engine = tauri::async_runtime::block_on(PieEngine::new())?;
```

with:

```rust
            let engine = {
                let settings = Settings::load();
                tauri::async_runtime::block_on(PieEngine::with_config(&llm_config(&settings)))?
            };
```

(If `settings` is already loaded earlier in the setup closure — check `main.rs:845` where `Settings::load()` is called — reuse that binding instead of loading twice.)

- [ ] **Step 3: Rebuild the router when LLM settings change**

In `update_settings` (`src-tauri/src/main.rs:380`), after the hotkey block and before/after `settings.save()`, detect an LLM-field change and rebuild. Replace the body's tail so it reads:

```rust
    let llm_changed = {
        let current = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        current.llm_api_url != settings.llm_api_url
            || current.llm_api_key != settings.llm_api_key
            || current.llm_model != settings.llm_model
    };
    settings.save().map_err(|e| e.to_string())?;
    if llm_changed {
        let cfg = llm_config(&settings);
        let mut engine = state.engine.lock().await;
        engine.set_llm_config(&cfg);
    }
    *state.settings.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
```

Note: `update_settings` must be `async` to `.await` the engine lock. It is currently sync (`fn update_settings`). Change its signature to `async fn update_settings(...)`. Tauri supports async commands; the `generate_handler!` registration needs no change.

- [ ] **Step 4: Add the `test_llm_connection` command**

Add near the other commands (e.g. after `send_to_llm` around `src-tauri/src/main.rs:589`):

```rust
/// Verify typed-but-unsaved LLM credentials with a minimal round-trip.
/// Returns Ok("connected") on success, Err(message) otherwise.
#[tauri::command]
async fn test_llm_connection(url: String, key: String, model: String) -> Result<String, String> {
    if url.trim().is_empty() {
        return Err("API URL is required".to_string());
    }
    let client = pie_engine::llm::openai::OpenAiClient::new(&url, &key);
    let model = if model.trim().is_empty() { "gpt-4o-mini".to_string() } else { model };
    match client.chat("ping", &model).await {
        Ok(_) => Ok("connected".to_string()),
        Err(e) => Err(e.to_string()),
    }
}
```

Confirm `pie_engine::llm::openai::OpenAiClient` is public (it is: `pub mod openai;` + `pub struct OpenAiClient` + `pub fn new`).

- [ ] **Step 5: Register the command**

In the `tauri::generate_handler![ … ]` list (around `src-tauri/src/main.rs:927`), add `test_llm_connection,` alongside `send_to_llm,`.

- [ ] **Step 6: Build to verify it compiles**

Run: `cargo build -p pie`
Expected: builds clean (no errors). If `cargo build -p pie` fails on package name, run `cargo build` from `src-tauri/`.

- [ ] **Step 7: Commit**

```bash
cargo fmt && cargo clippy
git add src-tauri/src/main.rs
git commit -m "feat(app): wire BYOK config to engine + test_llm_connection command"
```

---

### Task 5: `LLMSettings.svelte` UI section

**Files:**
- Create: `ui/src/lib/LLMSettings.svelte`
- Modify: `ui/src/App.svelte`

**Interfaces:**
- Consumes: `get_settings`/`update_settings` (via the `{settings}` prop + `onSave` callback pattern used by the other settings components), and `test_llm_connection` command (Task 4).
- Follows the existing component contract: `export let settings; export let onSave;` and calls `onSave()` to persist (matches `TranscriptionSettings`, `OutputSettings`, etc.).

- [ ] **Step 1: Read a sibling for the exact prop/style contract**

Read `ui/src/lib/OutputSettings.svelte` in full to copy its `<script>`/`<section class="group">`/`.field` structure, prop names, and save pattern. Match it — do not invent new class names.

- [ ] **Step 2: Create `LLMSettings.svelte`**

Create `ui/src/lib/LLMSettings.svelte` mirroring the sibling's structure. Content:

```svelte
<script>
  import { invoke } from "@tauri-apps/api/core";

  let { settings, onSave } = $props();

  let testing = $state(false);
  let testResult = $state("");

  async function testConnection() {
    testing = true;
    testResult = "";
    try {
      await invoke("test_llm_connection", {
        url: settings.llm_api_url,
        key: settings.llm_api_key,
        model: settings.llm_model,
      });
      testResult = "✓ Connected";
    } catch (e) {
      testResult = "✗ " + e;
    } finally {
      testing = false;
    }
  }
</script>

<section class="group">
  <div class="field">
    <span class="field-label">LLM Provider</span>
    <input
      placeholder="API URL (e.g. https://api.openai.com/v1)"
      bind:value={settings.llm_api_url}
      onchange={onSave} />
    <input
      type="password"
      placeholder="API Key"
      bind:value={settings.llm_api_key}
      onchange={onSave} />
    <input
      placeholder="Model (e.g. gpt-4o-mini)"
      bind:value={settings.llm_model}
      onchange={onSave} />
    <button class="btn sm" disabled={testing} onclick={testConnection}>
      {testing ? "Testing…" : "Test Connection"}
    </button>
    {#if testResult}
      <span class="caption">{testResult}</span>
    {/if}
  </div>
</section>
```

Adjust `$props()`/`$state()` runes vs. `export let` to whatever the sibling actually uses — if `OutputSettings.svelte` uses `export let`, use `export let settings, onSave;` here too, for consistency.

- [ ] **Step 3: Mount it in `App.svelte`**

In `ui/src/App.svelte`: add the import next to the others (around line 9):

```js
  import LLMSettings from "./lib/LLMSettings.svelte";
```

and render it in the settings layout next to `OutputSettings` (around line 233):

```svelte
    <LLMSettings {settings} onSave={save} />
```

- [ ] **Step 4: Build the frontend to verify it compiles**

Run (from `ui/`): `npm run build`
Expected: build succeeds with no Svelte compile errors.

- [ ] **Step 5: Manual smoke test (end-to-end)**

Run the app (`cargo tauri dev` from `src-tauri/`, or the project's usual dev command). In Settings:
1. Enter your API URL, key, and model. Confirm the key field masks input.
2. Click **Test Connection** → expect "✓ Connected" with valid creds, "✗ …" with a bad key.
3. Confirm values persist after closing/reopening settings (`get_settings` returns them).
4. Record a clip and run "Send to LLM" → confirm it uses the configured endpoint (no env vars set).

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/LLMSettings.svelte ui/src/App.svelte
git commit -m "feat(ui): LLM Provider settings section with Test Connection"
```

---

## Acceptance (Phase 1 spec)

- [ ] User sets URL + key + model in the UI and it persists (Tasks 2, 5).
- [ ] Test Connection verifies the endpoint, success and failure both visible (Tasks 4, 5).
- [ ] Router uses settings config when present, env vars when URL is empty (Tasks 1, 3).
- [ ] `pie --provider openai` (env path) still works — unchanged `new()`/`from_env` fallback (Task 1).
- [ ] API key uses a password input; never rendered in plaintext (Task 5).
```
