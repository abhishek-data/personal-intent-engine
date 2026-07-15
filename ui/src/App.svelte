<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import RecordingView from "./lib/RecordingView.svelte";
  import ModelManager from "./lib/ModelManager.svelte";
  import TranscriptionSettings from "./lib/TranscriptionSettings.svelte";
  import OutputSettings from "./lib/OutputSettings.svelte";
  import HotkeyRecorder from "./lib/HotkeyRecorder.svelte";

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

  // Model catalog + in-flight downloads (id -> {received, total}).
  let models = $state([]);
  let downloads = $state({});

  async function loadModels() {
    try {
      models = await invoke("list_models");
    } catch (e) {
      error = String(e);
    }
  }

  async function downloadModel(id) {
    error = "";
    downloads = { ...downloads, [id]: { received: 0, total: 0 } };
    try {
      await invoke("download_model", { id });
    } catch (e) {
      error = String(e);
    }
  }

  async function selectModel(id) {
    try {
      await invoke("select_model", { id });
      settings = await invoke("get_settings");
      await loadModels();
    } catch (e) {
      error = String(e);
    }
  }

  // Synchronous onMount so it can return a cleanup function. The listeners are
  // registered inside an async IIFE; `disposed` guards the race where the
  // component unmounts before `listen()` resolves, and the returned function
  // tears every subscription down so hot-reload / remount can't accumulate
  // duplicate handlers.
  onMount(() => {
    let unlisteners = [];
    let disposed = false;

    (async () => {
      try {
        settings = await invoke("get_settings");
      } catch (e) {
        error = String(e);
      }
      await loadModels();

      const subs = await Promise.all([
        listen("pie://download", (event) => {
          const p = event.payload;
          if (p.done) {
            const next = { ...downloads };
            delete next[p.id];
            downloads = next;
            if (p.error) error = p.error;
            loadModels();
          } else {
            downloads = {
              ...downloads,
              [p.id]: { received: p.received, total: p.total },
            };
          }
        }),
        listen("pie://models-changed", () => loadModels()),
        listen("pie://state", (event) => {
          recState = event.payload;
          if (recState === "recording") view = "record";
        }),
        listen("pie://outcome", (event) => {
          outcome = event.payload;
          llmResponse = "";
          view = "record";
        }),
        listen("pie://error", (event) => {
          error = String(event.payload);
        }),
      ]);

      if (disposed) {
        subs.forEach((u) => u());
      } else {
        unlisteners = subs;
      }
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
      clearTimeout(savedTimer);
    };
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
              aria-current={view === item.id ? "page" : undefined}
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
          <div class="banner error" role="alert">{error}</div>
        {/if}

        {#if view === "record"}
          <RecordingView
            {recState}
            {outcome}
            {llmResponse}
            {llmBusy}
            {stateLabel}
            hotkey={settings.hotkey}
            onToggle={toggleRecording}
            onCancel={cancelRecording}
            onSend={sendToLlm}
            onCopy={copyPrompt}
          />
        {:else if view === "models"}
          <ModelManager
            {models}
            {downloads}
            {settings}
            onDownload={downloadModel}
            onSelect={selectModel}
            onSave={save}
            onReloadModels={loadModels}
          />
        {:else if view === "transcription"}
          <TranscriptionSettings {settings} onSave={save} />
        {:else if view === "output"}
          <OutputSettings {settings} onSave={save} />
        {:else if view === "shortcut"}
          <HotkeyRecorder
            {settings}
            onSave={save}
            onError={(e) => (error = e)}
          />
        {/if}
      </div>
    </main>
  </div>
</div>
