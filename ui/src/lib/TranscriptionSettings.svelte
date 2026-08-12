<script>
  // Transcription pane: spoken language + prompt optimization mode.
  let { settings, onSave } = $props();

  const MODES = ["auto", "direct", "enhanced"];
</script>

<section class="leaf">
  <div class="leaf-head">
    <span class="leaf-label">Transcription</span>
    <span class="leaf-rule"></span>
  </div>

  <div class="field">
    <label for="language">Spoken language</label>
    <input
      id="language"
      bind:value={settings.language}
      onblur={onSave}
      placeholder="auto"
    />
    <p class="note">
      ISO code like <code>en</code>, <code>de</code>, <code>es</code> — or
      <code>auto</code> to detect.
    </p>
  </div>

  <div class="field">
    <span class="field-label">Optimization</span>
    <div class="options">
      {#each MODES as m}
        <button
          class="option"
          class:active={settings.mode === m}
          aria-pressed={settings.mode === m}
          onclick={() => {
            settings.mode = m;
            onSave();
          }}>{m}</button
        >
      {/each}
    </div>
    <p class="note">
      How speech becomes a prompt: direct passes short commands straight
      through, enhanced uses your LLM to extract intent from long or rambling
      dictation, auto picks per input.
    </p>
  </div>
</section>
