<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  // Recording state machine (from OpenSuperWhisper's indicator):
  // idle -> recording -> decoding -> idle, with error as a transient state.
  let recState = $state("idle");
  let error = $state("");
  let outcome = $state(null); // { transcript, objective, conversation_type, confidence, optimized_prompt, estimated_tokens, mode }
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
  let showSettings = $state(false);
  let settingsSaved = $state(false);

  onMount(async () => {
    try {
      settings = await invoke("get_settings");
    } catch (e) {
      error = String(e);
    }
    await listen("pie://state", (event) => {
      recState = event.payload;
    });
    // Hotkey-driven sessions report results and errors via events.
    await listen("pie://outcome", (event) => {
      outcome = event.payload;
      llmResponse = "";
    });
    await listen("pie://error", (event) => {
      error = String(event.payload);
    });
  });

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
    // decoding: button disabled, nothing to do
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

  async function saveSettings() {
    error = "";
    try {
      await invoke("update_settings", { settings });
      settingsSaved = true;
      setTimeout(() => (settingsSaved = false), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  const stateLabel = $derived(
    {
      idle: "Ready",
      recording: "Recording — click to stop",
      decoding: "Transcribing…",
      error: "Error",
    }[recState] ?? recState,
  );
</script>

<main>
  <header>
    <h1>PIE</h1>
    <span class="tagline">Personal Intent Engine</span>
    <button class="ghost" onclick={() => (showSettings = !showSettings)}>
      {showSettings ? "Close" : "Settings"}
    </button>
  </header>

  {#if showSettings}
    <section class="card settings">
      <label>
        Whisper model path
        <input bind:value={settings.whisper_model} placeholder="~/.cache/pie/models/ggml-tiny.en.bin" />
      </label>
      <label>
        Silero VAD model path
        <input bind:value={settings.silero_model} placeholder="~/.cache/pie/models/silero_vad_v4.onnx" />
      </label>
      <div class="row">
        <label>
          Language
          <input bind:value={settings.language} placeholder="auto" />
        </label>
        <label>
          Mode
          <select bind:value={settings.mode}>
            <option value="compact">compact</option>
            <option value="balanced">balanced</option>
            <option value="enhanced">enhanced</option>
            <option value="adaptive">adaptive</option>
          </select>
        </label>
        <label>
          Provider
          <select bind:value={settings.provider}>
            <option value="echo">echo (debug)</option>
            <option value="openai">openai</option>
            <option value="openrouter">openrouter</option>
          </select>
        </label>
        <label>
          LLM model
          <input bind:value={settings.llm_model} placeholder="gpt-4o-mini" />
        </label>
      </div>
      <div class="row">
        <label>
          Global hotkey (toggle recording from any app)
          <input bind:value={settings.hotkey} placeholder="CmdOrCtrl+Shift+Space" />
        </label>
        <label>
          Hotkey pastes
          <select bind:value={settings.paste_output}>
            <option value="transcript">transcript (raw speech)</option>
            <option value="prompt">optimized prompt</option>
          </select>
        </label>
      </div>
      <button onclick={saveSettings}>
        {settingsSaved ? "Saved ✓" : "Save settings"}
      </button>
    </section>
  {/if}

  <section class="recorder">
    <button
      class="record {recState}"
      onclick={toggleRecording}
      disabled={recState === "decoding"}
      aria-label={stateLabel}
    >
      <span class="dot"></span>
    </button>
    <p class="state-label">{stateLabel}</p>
    <p class="hint">or press <kbd>{settings.hotkey}</kbd> in any app</p>
    {#if recState === "recording"}
      <button class="ghost" onclick={cancelRecording}>Cancel</button>
    {/if}
  </section>

  {#if error}
    <section class="card error-card">{error}</section>
  {/if}

  {#if outcome}
    <section class="card">
      <h2>Transcript</h2>
      <p class="transcript">{outcome.transcript}</p>
      <div class="meta">
        <span>{outcome.conversation_type}</span>
        <span>{outcome.confidence} confidence</span>
        <span>{outcome.mode} mode</span>
        <span>~{outcome.estimated_tokens} tokens</span>
      </div>
    </section>

    <section class="card">
      <h2>Optimized prompt</h2>
      <pre>{outcome.optimized_prompt}</pre>
      <button onclick={sendToLlm} disabled={llmBusy}>
        {llmBusy ? "Sending…" : "Send to LLM"}
      </button>
    </section>
  {/if}

  {#if llmResponse}
    <section class="card">
      <h2>Response</h2>
      <pre>{llmResponse}</pre>
    </section>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    background: #101014;
    color: #e8e8ec;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
    letter-spacing: 0.04em;
  }
  .tagline {
    color: #8a8a94;
    font-size: 0.85rem;
    flex: 1;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #8a8a94;
  }
  .card {
    background: #18181e;
    border: 1px solid #26262e;
    border-radius: 10px;
    padding: 1rem;
  }
  .recorder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 1.5rem 0;
  }
  .record {
    width: 84px;
    height: 84px;
    border-radius: 50%;
    border: 2px solid #3a3a44;
    background: #18181e;
    cursor: pointer;
    display: grid;
    place-items: center;
    transition: border-color 0.2s;
  }
  .record .dot {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: #e5484d;
    transition: border-radius 0.2s;
  }
  .record.recording {
    border-color: #e5484d;
  }
  .record.recording .dot {
    border-radius: 6px;
    animation: blink 1.2s ease-in-out infinite;
  }
  .record.decoding {
    border-color: #f0b429;
    cursor: wait;
  }
  .record.decoding .dot {
    background: #f0b429;
    animation: pulse 0.9s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.4;
    }
  }
  @keyframes pulse {
    50% {
      transform: scale(0.75);
    }
  }
  .state-label {
    margin: 0;
    color: #8a8a94;
    font-size: 0.9rem;
  }
  .hint {
    margin: 0;
    color: #55555e;
    font-size: 0.78rem;
  }
  kbd {
    background: #26262e;
    border-radius: 4px;
    padding: 0.1rem 0.35rem;
    font-size: 0.75rem;
  }
  .transcript {
    margin: 0 0 0.75rem;
    font-size: 1.05rem;
    line-height: 1.5;
  }
  .meta {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .meta span {
    background: #26262e;
    border-radius: 999px;
    padding: 0.15rem 0.6rem;
    font-size: 0.75rem;
    color: #a8a8b3;
  }
  pre {
    white-space: pre-wrap;
    word-break: break-word;
    background: #101014;
    border-radius: 8px;
    padding: 0.75rem;
    font-size: 0.85rem;
    line-height: 1.45;
    margin: 0 0 0.75rem;
  }
  button {
    background: #2f6feb;
    color: white;
    border: none;
    border-radius: 8px;
    padding: 0.5rem 1rem;
    font-size: 0.9rem;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.ghost {
    background: transparent;
    border: 1px solid #3a3a44;
    color: #a8a8b3;
  }
  .error-card {
    border-color: #e5484d;
    color: #ff9a9e;
    font-size: 0.9rem;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .settings label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: #8a8a94;
    flex: 1;
  }
  .settings input,
  .settings select {
    background: #101014;
    border: 1px solid #26262e;
    border-radius: 6px;
    color: #e8e8ec;
    padding: 0.45rem 0.6rem;
    font-size: 0.85rem;
  }
  .settings .row {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  .settings button {
    align-self: flex-start;
  }
</style>
