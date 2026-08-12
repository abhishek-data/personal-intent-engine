<script>
  let { settings, onSave } = $props();
</script>

<section class="leaf">
  <div class="leaf-head">
    <span class="leaf-label">History</span>
    <span class="leaf-rule"></span>
  </div>

  <div class="field">
    <label for="history-limit">Recordings to keep</label>
    <input
      id="history-limit"
      type="number"
      min="1"
      max="1000"
      bind:value={settings.history_limit}
      onblur={onSave}
    />
    <p class="note">Older recordings beyond this count are deleted automatically.</p>
  </div>
</section>
