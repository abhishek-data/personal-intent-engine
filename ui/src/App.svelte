<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import RecordingView from "./lib/RecordingView.svelte";
  import ModelManager from "./lib/ModelManager.svelte";
  import TranscriptionSettings from "./lib/TranscriptionSettings.svelte";
  import OutputSettings from "./lib/OutputSettings.svelte";
  import HotkeyRecorder from "./lib/HotkeyRecorder.svelte";
  import HistorySettings from "./lib/HistorySettings.svelte";
  import HistoryView from "./lib/HistoryView.svelte";

  let view = $state("record");

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
    history_limit: 10,
  });
  let saved = $state(false);
  let savedTimer;

  let models = $state([]);
  let downloads = $state({});

  async function loadModels() {
    try { models = await invoke("list_models"); }
    catch (e) { error = String(e); }
  }

  async function downloadModel(id) {
    error = "";
    downloads = { ...downloads, [id]: { received: 0, total: 0 } };
    try { await invoke("download_model", { id }); }
    catch (e) { error = String(e); }
  }

  async function selectModel(id) {
    try {
      await invoke("select_model", { id });
      settings = await invoke("get_settings");
      await loadModels();
    } catch (e) { error = String(e); }
  }

  async function deleteModel(id) {
    error = "";
    try {
      await invoke("delete_model", { id });
      await loadModels();
    } catch (e) { error = String(e); }
  }

  async function save() {
    try {
      await invoke("update_settings", { settings });
      saved = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => { saved = false; }, 1500);
    } catch (e) { error = String(e); }
  }

  async function toggleRecording() {
    error = "";
    outcome = null;
    llmResponse = "";
    try {
      if (recState === "idle") {
        await invoke("start_recording");
      } else if (recState === "recording") {
        const result = await invoke("stop_recording");
        outcome = result;
      }
    } catch (e) { error = String(e); }
  }

  async function cancelRecording() {
    try { await invoke("cancel_recording"); }
    catch (e) { error = String(e); }
  }

  async function sendToLlm() {
    if (!outcome) return;
    llmBusy = true;
    llmResponse = "";
    try {
      llmResponse = await invoke("send_to_llm", {
        prompt: outcome.optimized_prompt,
      });
    } catch (e) { error = String(e); }
    finally { llmBusy = false; }
  }

  async function copyPrompt() {
    if (!outcome) return;
    try { await invoke("copy_to_clipboard", { text: outcome.optimized_prompt }); }
    catch (e) { error = String(e); }
  }

  const stateLabel = $derived(
    { idle: "Ready", recording: "Recording…", decoding: "Transcribing…" }[recState] ?? recState
  );

  const TABS = [
    { id: "record", label: "Record" },
    { id: "history", label: "History" },
    { id: "models", label: "Models" },
    { id: "settings", label: "Settings" },
  ];

  onMount(() => {
    let unlisteners = [];
    let disposed = false;

    (async () => {
      try { settings = await invoke("get_settings"); }
      catch (e) { error = String(e); }
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
            downloads = { ...downloads, [p.id]: { received: p.received, total: p.total } };
          }
        }),
        listen("pie://models-changed", () => loadModels()),
        listen("pie://state", (event) => { recState = event.payload; }),
        listen("pie://outcome", (event) => { outcome = event.payload; }),
        listen("pie://error", (event) => { error = event.payload; }),
      ]);

      if (disposed) { subs.forEach((u) => u()); return; }
      unlisteners = subs;
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  });
</script>

<svelte:window
  onkeydown={(e) => { if (e.key === "Escape" && recState === "recording") cancelRecording(); }}
/>

<div class="topbar">
  {#if saved}
    <span class="saved-tag">Saved ✓</span>
  {/if}
  <nav>
    {#each TABS as tab}
      <button
        class:active={view === tab.id}
        onclick={() => { view = tab.id; error = ""; }}
      >{tab.label}</button>
    {/each}
  </nav>
</div>

<div class="content">
  {#if error}
    <div class="error-banner">
      <span>{error}</span>
      <button onclick={() => { error = ""; }} aria-label="Dismiss error">×</button>
    </div>
  {/if}

  {#if view === "record"}
    <RecordingView
      {recState}
      {outcome}
      {llmResponse}
      {llmBusy}
      hotkey={settings.hotkey}
      {stateLabel}
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
      onDelete={deleteModel}
      onSave={save}
      onReloadModels={loadModels}
    />
  {:else if view === "history"}
    <HistoryView />
  {:else if view === "settings"}
    <TranscriptionSettings {settings} onSave={save} />
    <OutputSettings {settings} onSave={save} />
    <HotkeyRecorder {settings} onSave={save} onError={(e) => { error = e; }} />
    <HistorySettings {settings} onSave={save} />
  {/if}
</div>
