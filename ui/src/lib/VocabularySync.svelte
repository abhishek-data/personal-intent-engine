<script>
  // Vocabulary bootstrap: import terms from past AI conversations (Cursor
  // history, ChatGPT/Claude exports, or a plain folder of markdown/text) so
  // the learned-vocabulary dictionary starts warm instead of empty.
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let { settings, onSave, onError } = $props();

  let sources = $state([]);
  let paths = $state([]);
  let syncing = $state(false);
  let progress = $state({ done: 0, total: 0 });
  let result = $state(null);
  let lastRunUnix = $state(0);

  async function loadSources() {
    try { sources = await invoke("discover_sync_sources"); }
    catch (e) { onError(String(e)); }
  }

  async function refreshState() {
    try {
      const s = await invoke("get_sync_state");
      lastRunUnix = s.last_run_unix;
    } catch (e) { onError(String(e)); }
  }

  function addPath(p) {
    if (!p || paths.includes(p)) return;
    paths = [...paths, p];
  }

  function removePath(p) {
    paths = paths.filter((x) => x !== p);
  }

  async function chooseFolder() {
    try { addPath(await invoke("pick_import_target", { folder: true })); }
    catch (e) { onError(String(e)); }
  }

  async function chooseFile() {
    try { addPath(await invoke("pick_import_target", { folder: false })); }
    catch (e) { onError(String(e)); }
  }

  async function syncNow() {
    if (paths.length === 0 || syncing) return;
    syncing = true;
    result = null;
    progress = { done: 0, total: 0 };
    try {
      const res = await invoke("run_vocabulary_sync", { paths });
      if (res.batches_total > 0 && res.batches_failed === res.batches_total) {
        // Every batch failed: this is a failure, not "0 terms found".
        onError(
          "Vocabulary sync failed — the LLM returned an error for every batch. Check your LLM provider settings."
        );
      } else {
        result = res;
      }
      await refreshState();
    } catch (e) {
      onError(String(e));
    } finally {
      syncing = false;
    }
  }

  const lastSyncedLabel = $derived(
    lastRunUnix ? new Date(lastRunUnix * 1000).toLocaleString() : "never"
  );
  const progressPct = $derived(
    progress.total ? Math.round((progress.done / progress.total) * 100) : 0
  );

  onMount(() => {
    let unlisten;
    let disposed = false;

    loadSources();
    refreshState();

    (async () => {
      const u = await listen("sync-progress", (event) => {
        const [done, total] = event.payload;
        progress = { done, total };
      });
      if (disposed) { u(); return; }
      unlisten = u;
    })();

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  });
</script>

<section class="group">
  <span class="field-label">Sync Your Vocabulary</span>
  <p class="caption">
    All processing happens locally. Text is only sent to the LLM you've
    configured, to extract vocabulary terms from your past conversations.
  </p>

  <div class="field">
    <span class="field-label">Auto-detected sources</span>
    {#if sources.length}
      <ul class="correction-list">
        {#each sources as s (s.path)}
          <li>
            <span class="mono">✓ {s.name}</span>
            <button
              class="text-btn"
              onclick={() => addPath(s.path)}
              aria-label={`Add ${s.name}`}
            >Add</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption">No sources auto-detected on this machine.</p>
    {/if}
  </div>

  <div class="field">
    <span class="field-label">Sources to sync</span>
    <div class="correction-add">
      <button class="btn sm" onclick={chooseFolder}>Choose folder…</button>
      <button class="btn sm" onclick={chooseFile}>Choose file…</button>
    </div>
    {#if paths.length}
      <ul class="correction-list">
        {#each paths as p (p)}
          <li>
            <span class="mono">{p}</span>
            <button
              class="text-btn"
              onclick={() => removePath(p)}
              aria-label={`Remove ${p}`}
            >✕</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption">
        No sources chosen yet. Pick a folder of exported conversations (or an
        export file, e.g. ChatGPT/Claude `conversations.json`).
      </p>
    {/if}
  </div>

  <div class="field">
    <button
      class="btn sm"
      disabled={paths.length === 0 || syncing}
      onclick={syncNow}
    >{syncing ? "Syncing…" : "Sync Now"}</button>

    {#if syncing}
      <div
        class="progress"
        style="width:100%; margin-top:var(--space-2)"
        title="{progressPct}%"
      >
        <div class="progress-bar" style="width:{progressPct}%"></div>
      </div>
      <p class="caption">{progress.done} / {progress.total} conversations processed</p>
    {/if}

    {#if result}
      <p class="caption">
        Imported {result.terms_added} terms from {result.conversations} conversations.
      </p>
      {#if result.batches_failed > 0}
        <p class="caption">
          ⚠ {result.batches_failed} of {result.batches_total} batches failed (check your LLM settings)
        </p>
      {/if}
    {/if}

    <p class="caption">Last synced: {lastSyncedLabel}</p>
  </div>
</section>
