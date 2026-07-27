<script>
  // Output pane: default output mode for window-initiated recordings.
  // (The two global hotkeys have their own fixed outputs.)
  let { settings, onSave } = $props();
</script>

<section class="group">
  <div class="field">
    <span class="field-label">Record-window default</span>
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
      Default output mode for recordings started from this window (long prompts
      are only refined for “Optimized prompt”). The two global hotkeys have their
      own fixed outputs — raw vs optimized — and ignore this.
    </p>
  </div>
</section>
