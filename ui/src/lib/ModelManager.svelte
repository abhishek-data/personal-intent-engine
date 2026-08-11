<script>
  let { models, downloads, settings, onDownload, onSelect, onDelete, onSave, onReloadModels } = $props();
  let showCustomPaths = $state(false);
</script>

{#snippet modelRow(m)}
  <div class="model" class:selected={m.selected}>
    <div class="model-info">
      <span class="model-name">
        {m.name}
        {#if m.selected}<span class="badge">In use</span>{/if}
      </span>
      <span class="model-desc">{m.description} · {m.size_mb} MB</span>
    </div>
    <div class="model-action">
      {#if downloads[m.id]}
        {@const d = downloads[m.id]}
        {@const p = d.total ? Math.round((d.received / d.total) * 100) : 0}
        <div class="progress" title="{p}%">
          <div class="progress-bar" style="width:{p}%"></div>
        </div>
        <span class="model-pct">{p}%</span>
      {:else if !m.downloaded}
        <button class="btn sm" onclick={() => onDownload(m.id)} aria-label="Download {m.name}">Download</button>
      {:else if m.selected}
        <button class="btn ghost sm" disabled>Selected</button>
      {:else}
        <div class="model-actions-row">
          <button class="btn sm" onclick={() => onSelect(m.id)} aria-label="Use {m.name}">Use</button>
          <button class="btn ghost sm delete-btn" onclick={() => onDelete(m.id)} aria-label="Delete {m.name}" title="Delete model">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
              <path d="M2.5 4.5h11M5.5 4.5V3a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1.5M6.5 7v4.5M9.5 7v4.5M3.5 4.5l.7 8.1a1.5 1.5 0 0 0 1.5 1.4h4.6a1.5 1.5 0 0 0 1.5-1.4l.7-8.1"/>
            </svg>
          </button>
        </div>
      {/if}
    </div>
  </div>
{/snippet}

<section class="group">
  <span class="group-eyebrow">Speech to text</span>
  {#each models.filter((m) => m.kind === "whisper") as m}
    {@render modelRow(m)}
  {/each}
</section>

<section class="group">
  <span class="group-eyebrow">Voice detection</span>
  {#each models.filter((m) => m.kind === "vad") as m}
    {@render modelRow(m)}
  {/each}
  <p class="caption">Optional. Trims silence so only speech is transcribed.</p>
</section>

<details class="disclosure" bind:open={showCustomPaths}>
  <summary>Custom model paths</summary>
  <div class="group" style="margin-top:var(--space-3)">
    <div class="field">
      <label for="whisper">Whisper model path</label>
      <input
        id="whisper"
        bind:value={settings.whisper_model}
        onblur={() => { onSave(); onReloadModels(); }}
        placeholder="~/.cache/pie/models/ggml-tiny.en.bin"
      />
    </div>
    <div class="field">
      <label for="silero">Voice detection model path</label>
      <input
        id="silero"
        bind:value={settings.silero_model}
        onblur={() => { onSave(); onReloadModels(); }}
        placeholder="~/.cache/pie/models/silero_vad_v4.onnx"
      />
    </div>
    <p class="caption">Override the catalog with your own model files.</p>
  </div>
</details>
