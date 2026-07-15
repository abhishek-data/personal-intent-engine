<script>
  // Transcription pane: spoken language + prompt optimization mode.
  let { settings, onSave } = $props();

  const MODES = ["compact", "balanced", "enhanced", "adaptive"];
</script>

<section class="group">
  <div class="field">
    <label for="language">Spoken language</label>
    <input
      id="language"
      bind:value={settings.language}
      onblur={onSave}
      placeholder="auto"
    />
    <p class="caption">
      ISO code like <code>en</code>, <code>de</code>, <code>es</code> — or
      <code>auto</code> to detect.
    </p>
  </div>
  <div class="field">
    <span class="field-label">Optimization</span>
    <div class="segmented">
      {#each MODES as m}
        <button
          class:active={settings.mode === m}
          aria-pressed={settings.mode === m}
          onclick={() => {
            settings.mode = m;
            onSave();
          }}>{m}</button
        >
      {/each}
    </div>
    <p class="caption">
      How speech becomes a prompt: compact strips filler, enhanced enriches
      context, adaptive picks per input.
    </p>
  </div>
</section>
