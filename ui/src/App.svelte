<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  // Which section of the app is showing. "record" is home; the rest are
  // settings groups, mirroring OpenSuperWhisper's sidebar sections.
  let view = $state("record");

  // Recording state machine (idle -> recording -> decoding -> idle),
  // with "error" as a transient state.
  let recState = $state("idle");
  let error = $state("");
  let outcome = $state(null);
  let llmResponse = $state("");
  let llmBusy = $state(false);

  let settings = $state({
    whisper_model: "",
    silero_model: "",
    language: "auto",
    mode: "balanced",
    provider: "echo",
    llm_model: "",
    hotkey: "CmdOrCtrl+Shift+Space",
    paste_output: "transcript",
  });
  let saved = $state(false);
  let savedTimer;

  onMount(async () => {
    try {
      settings = await invoke("get_settings");
    } catch (e) {
      error = String(e);
    }
    await listen("pie://state", (event) => {
      recState = event.payload;
      if (recState === "recording") view = "record";
    });
    await listen("pie://outcome", (event) => {
      outcome = event.payload;
      llmResponse = "";
      view = "record";
    });
    await listen("pie://error", (event) => {
      error = String(event.payload);
    });
  });

  async function save() {
    error = "";
    try {
      await invoke("update_settings", { settings: $state.snapshot(settings) });
      saved = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (saved = false), 1400);
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleRecording() {
    error = "";
    if (recState === "idle" || recState === "error") {
      outcome = null;
      llmResponse = "";
      try {
        await invoke("start_recording");
      } catch (e) {
        recState = "error";
        error = String(e);
      }
    } else if (recState === "recording") {
      try {
        outcome = await invoke("stop_recording");
      } catch (e) {
        recState = "error";
        error = String(e);
      }
    }
  }

  async function cancelRecording() {
    try {
      await invoke("cancel_recording");
    } catch (e) {
      error = String(e);
    }
  }

  async function sendToLlm() {
    if (!outcome) return;
    llmBusy = true;
    llmResponse = "";
    error = "";
    try {
      llmResponse = await invoke("send_to_llm", {
        prompt: outcome.optimized_prompt,
      });
    } catch (e) {
      error = String(e);
    } finally {
      llmBusy = false;
    }
  }

  async function copyPrompt() {
    if (outcome) await navigator.clipboard.writeText(outcome.optimized_prompt);
  }

  const stateLabel = $derived(
    {
      idle: "Ready",
      recording: "Listening — click to stop",
      decoding: "Transcribing…",
      error: "Something went wrong",
    }[recState] ?? recState,
  );

  const nav = [
    { id: "record", label: "Record", icon: "mic" },
    { id: "models", label: "Models", icon: "chip" },
    { id: "transcription", label: "Transcription", icon: "wave" },
    { id: "output", label: "Output", icon: "output" },
    { id: "shortcut", label: "Shortcut", icon: "command" },
  ];
  const sectionTitle = $derived(nav.find((n) => n.id === view)?.label ?? "");
</script>

<!-- Inline stroke icons (no icon font, no network) -->
{#snippet icon(name)}
  <svg
    class="nav-icon"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    {#if name === "mic"}
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M6 11a6 6 0 0 0 12 0" />
      <line x1="12" y1="17" x2="12" y2="21" />
    {:else if name === "chip"}
      <rect x="7" y="7" width="10" height="10" rx="2" />
      <path d="M10 4v3M14 4v3M10 17v3M14 17v3M4 10h3M4 14h3M17 10h3M17 14h3" />
    {:else if name === "wave"}
      <path d="M4 12h2M9 7v10M14 4v16M19 9v6M21 11v2" />
    {:else if name === "output"}
      <path d="M10 8V6a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2v-2" />
      <path d="M3 12h11M11 8l4 4-4 4" />
    {:else if name === "command"}
      <path
        d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z"
      />
    {/if}
  </svg>
{/snippet}

<div class="app">
  <div class="titlebar" data-tauri-drag-region></div>

  <div class="body">
    <nav class="sidebar">
      <div class="brand">
        <span class="mark">◐</span>
        <div class="brand-text">
          <span class="brand-name">PIE</span>
          <span class="brand-sub">Personal Intent Engine</span>
        </div>
      </div>
      <ul class="nav">
        {#each nav as item}
          <li>
            <button
              class="nav-item"
              class:active={view === item.id}
              onclick={() => (view = item.id)}
            >
              {@render icon(item.icon)}
              {item.label}
            </button>
          </li>
        {/each}
      </ul>
      <div class="sidebar-foot">
        <span class="status-dot {recState}"></span>
        {stateLabel}
      </div>
    </nav>

    <main class="detail">
      <header class="detail-head">
        <h1>{sectionTitle}</h1>
        {#if saved}<span class="saved">Saved</span>{/if}
      </header>

      <div class="detail-body">
        {#if error}
          <div class="banner error">{error}</div>
        {/if}

        {#if view === "record"}
          <section class="record-hero" class:centered={!outcome}>
            <button
              class="record {recState}"
              onclick={toggleRecording}
              disabled={recState === "decoding"}
              aria-label={stateLabel}
            >
              <span class="dot"></span>
            </button>
            <p class="record-state">{stateLabel}</p>
            <p class="record-hint">
              or press <kbd>{settings.hotkey}</kbd> in any app
            </p>
            {#if recState === "recording"}
              <button class="text-btn" onclick={cancelRecording}>Cancel</button>
            {/if}
          </section>

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
                  <span class="muted"
                    >{outcome.mode} · ~{outcome.estimated_tokens} tokens</span
                  >
                </div>
                <pre class="prompt">{outcome.optimized_prompt}</pre>
                <div class="actions">
                  <button class="btn" onclick={sendToLlm} disabled={llmBusy}>
                    {llmBusy ? "Sending…" : "Send to LLM"}
                  </button>
                  <button class="btn ghost" onclick={copyPrompt}>Copy</button>
                </div>
              </div>

              {#if llmResponse}
                <div class="result-step">
                  <span class="eyebrow">Response</span>
                  <pre class="response">{llmResponse}</pre>
                </div>
              {/if}
            </section>
          {/if}
        {/if}

        {#if view === "models"}
          <section class="group">
            <div class="field">
              <label for="whisper">Whisper model</label>
              <input
                id="whisper"
                bind:value={settings.whisper_model}
                onblur={save}
                placeholder="~/.cache/pie/models/ggml-tiny.en.bin"
              />
              <p class="caption">
                GGML/GGUF speech-to-text model. Larger models are more accurate
                but slower.
              </p>
            </div>
            <div class="field">
              <label for="silero">Voice detection model</label>
              <input
                id="silero"
                bind:value={settings.silero_model}
                onblur={save}
                placeholder="~/.cache/pie/models/silero_vad_v4.onnx"
              />
              <p class="caption">
                Silero VAD trims silence so only speech is transcribed. Leave
                empty to record continuously.
              </p>
            </div>
          </section>
        {/if}

        {#if view === "transcription"}
          <section class="group">
            <div class="field">
              <label for="language">Spoken language</label>
              <input
                id="language"
                bind:value={settings.language}
                onblur={save}
                placeholder="auto"
              />
              <p class="caption">
                ISO code like <code>en</code>, <code>de</code>,
                <code>es</code> — or <code>auto</code> to detect.
              </p>
            </div>
            <div class="field">
              <span class="field-label">Optimization</span>
              <div class="segmented">
                {#each ["compact", "balanced", "enhanced", "adaptive"] as m}
                  <button
                    class:active={settings.mode === m}
                    onclick={() => {
                      settings.mode = m;
                      save();
                    }}>{m}</button
                  >
                {/each}
              </div>
              <p class="caption">
                How speech becomes a prompt: compact strips filler, enhanced
                enriches context, adaptive picks per input.
              </p>
            </div>
          </section>
        {/if}

        {#if view === "output"}
          <section class="group">
            <div class="field">
              <span class="field-label">Hotkey pastes</span>
              <div class="segmented">
                <button
                  class:active={settings.paste_output === "transcript"}
                  onclick={() => {
                    settings.paste_output = "transcript";
                    save();
                  }}>Transcript</button
                >
                <button
                  class:active={settings.paste_output === "prompt"}
                  onclick={() => {
                    settings.paste_output = "prompt";
                    save();
                  }}>Optimized prompt</button
                >
              </div>
              <p class="caption">
                What lands in the focused app after the hotkey: raw speech, or
                the PIE-structured prompt.
              </p>
            </div>
            <div class="row">
              <div class="field">
                <label for="provider">LLM provider</label>
                <select
                  id="provider"
                  bind:value={settings.provider}
                  onchange={save}
                >
                  <option value="echo">Echo (debug)</option>
                  <option value="openai">OpenAI</option>
                  <option value="openrouter">OpenRouter</option>
                </select>
              </div>
              <div class="field">
                <label for="llm-model">Model</label>
                <input
                  id="llm-model"
                  bind:value={settings.llm_model}
                  onblur={save}
                  placeholder="gpt-4o-mini"
                />
              </div>
            </div>
            <p class="caption">
              Used by “Send to LLM”. OpenAI needs <code>OPENAI_API_KEY</code> in
              the environment.
            </p>
          </section>
        {/if}

        {#if view === "shortcut"}
          <section class="group">
            <div class="field">
              <label for="hotkey">Global hotkey</label>
              <input
                id="hotkey"
                bind:value={settings.hotkey}
                onblur={save}
                placeholder="CmdOrCtrl+Shift+Space"
              />
              <p class="caption">
                Press it in any app to start recording; press again to stop and
                paste. Leave empty to disable. First use needs Accessibility
                permission on macOS.
              </p>
            </div>
          </section>
        {/if}
      </div>
    </main>
  </div>
</div>

<style>
  :global(:root) {
    --bg: #141519;
    --sidebar: #101114;
    --card: #1c1e24;
    --card-hi: #22242b;
    --line: rgba(255, 255, 255, 0.07);
    --line-hi: rgba(255, 255, 255, 0.12);
    --text: #e7e8ec;
    --text-2: #9a9ba4;
    --text-3: #6b6c75;
    --accent: #6e79ff;
    --accent-hi: #838cff;
    --accent-dim: rgba(110, 121, 255, 0.14);
    --rec: #e5484d;
    --warn: #f0b429;
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  /* Draggable strip so the frameless window still moves; traffic lights
     overlay its left edge (titleBarStyle: Overlay). */
  .titlebar {
    height: 30px;
    flex-shrink: 0;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  /* ── sidebar ── */
  .sidebar {
    width: 208px;
    flex-shrink: 0;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: 0 10px 10px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.4rem 0.5rem 1.1rem;
  }
  .mark {
    font-size: 1.5rem;
    color: var(--accent);
    line-height: 1;
  }
  .brand-text {
    display: flex;
    flex-direction: column;
  }
  .brand-name {
    font-size: 0.95rem;
    font-weight: 700;
    letter-spacing: 0.06em;
  }
  .brand-sub {
    font-size: 0.66rem;
    color: var(--text-3);
    letter-spacing: 0.02em;
  }
  .nav {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.44rem 0.55rem;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-size: 0.83rem;
    cursor: default;
    text-align: left;
  }
  .nav-item:hover {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--accent-dim);
    color: var(--text);
  }
  .nav-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    opacity: 0.9;
  }
  .sidebar-foot {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    font-size: 0.72rem;
    color: var(--text-3);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #3a3b44;
  }
  .status-dot.recording {
    background: var(--rec);
    animation: blink 1.2s ease-in-out infinite;
  }
  .status-dot.decoding {
    background: var(--warn);
    animation: pulse 0.9s ease-in-out infinite;
  }

  /* ── detail ── */
  .detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .detail-head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.3rem 1.5rem 0.9rem;
    border-bottom: 1px solid var(--line);
  }
  .detail-head h1 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
  }
  .saved {
    font-size: 0.7rem;
    color: var(--accent-hi);
    background: var(--accent-dim);
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
  }
  .detail-body {
    flex: 1;
    overflow-y: auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  .banner {
    border-radius: 9px;
    padding: 0.7rem 0.9rem;
    font-size: 0.82rem;
  }
  .banner.error {
    background: rgba(229, 72, 77, 0.12);
    border: 1px solid rgba(229, 72, 77, 0.3);
    color: #ff9ea1;
  }

  /* ── record hero ── */
  .record-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.55rem;
    padding: 1.5rem 0 0.5rem;
  }
  /* When there's no result yet, center the button in the empty pane. */
  .record-hero.centered {
    flex: 1;
    justify-content: center;
    padding: 0;
  }
  .record {
    width: 88px;
    height: 88px;
    border-radius: 50%;
    border: 2px solid #33343d;
    background: radial-gradient(circle at 50% 38%, #23252c, #191a1f);
    cursor: default;
    display: grid;
    place-items: center;
    transition:
      border-color 0.2s,
      transform 0.1s;
  }
  .record:hover:not(:disabled) {
    border-color: #44454f;
  }
  .record:active:not(:disabled) {
    transform: scale(0.97);
  }
  .record .dot {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--rec);
    transition: border-radius 0.2s;
  }
  .record.recording {
    border-color: var(--rec);
  }
  .record.recording .dot {
    border-radius: 7px;
    animation: blink 1.2s ease-in-out infinite;
  }
  .record.decoding {
    border-color: var(--warn);
    cursor: wait;
  }
  .record.decoding .dot {
    background: var(--warn);
    animation: pulse 0.9s ease-in-out infinite;
  }
  .record-state {
    margin: 0;
    font-size: 0.9rem;
  }
  .record-hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--text-3);
  }

  /* ── result (the signature: speech → intent → prompt) ── */
  .result {
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
  }
  .result-step {
    padding: 1rem 1.1rem;
    border-top: 1px solid var(--line);
  }
  .result-step:first-child {
    border-top: none;
  }
  .eyebrow {
    display: block;
    font-size: 0.66rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.11em;
    color: var(--text-3);
    margin-bottom: 0.5rem;
  }
  .step-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .muted {
    font-size: 0.72rem;
    color: var(--text-3);
  }
  .transcript {
    margin: 0;
    font-size: 1.05rem;
    line-height: 1.5;
  }
  .chips {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .chip {
    background: var(--card-hi);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0.2rem 0.65rem;
    font-size: 0.75rem;
    color: var(--text-2);
  }
  .chip.objective {
    color: var(--text);
    border-color: var(--line-hi);
  }
  .prompt,
  .response {
    white-space: pre-wrap;
    word-break: break-word;
    background: #0e0f13;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.75rem;
    font-size: 0.82rem;
    line-height: 1.5;
    margin: 0;
    font-family: "SF Mono", ui-monospace, Menlo, monospace;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  /* ── buttons ── */
  .btn {
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 7px;
    padding: 0.45rem 0.9rem;
    font-size: 0.82rem;
    cursor: default;
  }
  .btn:hover:not(:disabled) {
    background: var(--accent-hi);
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .btn.ghost {
    background: transparent;
    border: 1px solid var(--line-hi);
    color: var(--text-2);
  }
  .btn.ghost:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.04);
  }
  .text-btn {
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 0.78rem;
    cursor: default;
    padding: 0.2rem;
  }
  .text-btn:hover {
    color: var(--text-2);
  }

  /* ── settings groups ── */
  .group {
    display: flex;
    flex-direction: column;
    gap: 1.4rem;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 1.25rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .field label,
  .field-label {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text);
  }
  .row {
    display: flex;
    gap: 1rem;
  }
  .row .field {
    flex: 1;
  }
  input,
  select {
    background: #0e0f13;
    border: 1px solid var(--line);
    border-radius: 7px;
    color: var(--text);
    padding: 0.5rem 0.65rem;
    font-size: 0.83rem;
    width: 100%;
    box-sizing: border-box;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-dim);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .caption {
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.45;
    color: var(--text-3);
  }
  .caption code,
  code {
    background: var(--card-hi);
    border-radius: 4px;
    padding: 0.05rem 0.3rem;
    font-size: 0.9em;
    font-family: "SF Mono", ui-monospace, Menlo, monospace;
    color: var(--text-2);
  }
  kbd {
    background: var(--card-hi);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 0.1rem 0.4rem;
    font-size: 0.72rem;
    font-family: inherit;
    color: var(--text-2);
  }

  /* ── segmented control ── */
  .segmented {
    display: inline-flex;
    background: #0e0f13;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 2px;
    gap: 2px;
    width: fit-content;
  }
  .segmented button {
    border: none;
    background: transparent;
    color: var(--text-2);
    padding: 0.35rem 0.75rem;
    border-radius: 6px;
    font-size: 0.78rem;
    cursor: default;
    text-transform: capitalize;
  }
  .segmented button:hover {
    color: var(--text);
  }
  .segmented button.active {
    background: var(--accent);
    color: white;
  }

  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
  @keyframes pulse {
    50% {
      transform: scale(0.7);
    }
  }
</style>
