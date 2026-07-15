<script>
  let { models, downloads, settings, onDownload, onSelect, onSave, onReloadModels } = $props();
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
        <button class="btn sm" onclick={() => onSelect(m.id)} aria-label="Use {m.name}">Use</button>
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
