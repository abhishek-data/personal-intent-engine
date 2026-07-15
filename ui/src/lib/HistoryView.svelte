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
