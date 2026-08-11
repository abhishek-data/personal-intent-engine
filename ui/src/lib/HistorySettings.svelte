<script>
  let { settings, onSave } = $props();
</script>

<section class="group">
  <span class="group-eyebrow">History</span>
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
    <p class="caption">Older recordings beyond this count are deleted automatically.</p>
  </div>
</section>
