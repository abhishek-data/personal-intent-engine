<script>
  // Output pane: what the hotkey pastes after transcription.
  let { settings, onSave } = $props();
</script>

<section class="group">
  <div class="field">
    <span class="field-label">Hotkey pastes</span>
    <div class="segmented">
      <button
        class:active={settings.paste_output === "transcript"}
        aria-pressed={settings.paste_output === "transcript"}
        onclick={() => {
          settings.paste_output = "transcript";
          onSave();
        }}>Transcript</button
      >
      <button
        class:active={settings.paste_output === "prompt"}
        aria-pressed={settings.paste_output === "prompt"}
        onclick={() => {
          settings.paste_output = "prompt";
          onSave();
        }}>Optimized prompt</button
      >
    </div>
    <p class="caption">
      What lands in the focused app after the hotkey: raw speech, or the
      PIE-structured prompt.
    </p>
  </div>
</section>
