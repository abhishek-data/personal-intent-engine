<script>
  // Vocabulary pane: opt-in AI deep-correct toggle, plus the user's own
  // heard→canonical corrections (the "pronunciation.json" dictionary).
  import { invoke } from "@tauri-apps/api/core";

  let { settings, onSave, onError } = $props();

  let corrections = $state([]);
  let heard = $state("");
  let canonical = $state("");

  async function refresh() {
    try { corrections = await invoke("list_corrections"); }
    catch (e) { onError(String(e)); }
  }
  refresh();

  async function add() {
    if (!heard.trim() || !canonical.trim()) return;
    try {
      await invoke("add_correction", { heard: heard.trim(), canonical: canonical.trim() });
      heard = "";
      canonical = "";
      await refresh();
    } catch (e) { onError(String(e)); }
  }

  async function remove(h) {
    try {
      await invoke("delete_correction", { heard: h });
      await refresh();
    } catch (e) { onError(String(e)); }
  }
</script>

<section class="group">
  <div class="field">
    <label class="toggle-row">
      <input
        type="checkbox"
        bind:checked={settings.deep_correct_ai}
        onchange={onSave}
      />
      <span>
        <span class="field-label toggle-label">Deep-correct with AI</span>
        <span class="caption toggle-caption">
          Use the configured LLM to fix garbled terms the dictionary misses.
          Slower, and uses your provider.
        </span>
      </span>
    </label>
  </div>

  <div class="field">
    <span class="field-label">Your corrections</span>
    <div class="correction-add">
      <input placeholder="heard (e.g. next jazz)" bind:value={heard} />
      <span class="arrow" aria-hidden="true">→</span>
      <input placeholder="correct (e.g. Next.js)" bind:value={canonical} />
      <button class="btn sm" onclick={add} aria-label="Add correction">Add</button>
    </div>
    {#if corrections.length}
      <ul class="correction-list">
        {#each corrections as c (c.heard)}
          <li>
            <span class="mono">{c.heard}</span>
            <span class="arrow" aria-hidden="true">→</span>
            <span class="mono">{c.canonical}</span>
            <button
              class="text-btn"
              onclick={() => remove(c.heard)}
              aria-label={`Delete correction for ${c.heard}`}
            >Delete</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption">No custom corrections yet. Add one above, or save one from a result.</p>
    {/if}
  </div>
</section>
