<script>
  // Output pane: what the hotkey pastes, and which LLM "Send to LLM" targets.
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
  <div class="row">
    <div class="field">
      <label for="provider">LLM provider</label>
      <select id="provider" bind:value={settings.provider} onchange={onSave}>
        <option value="echo">Echo (debug)</option>
        <option value="openai">OpenAI</option>
        <option value="openrouter">OpenRouter</option>
      </select>
    </div>
    <div class="field">
      <label for="llm-model">Model</label>
      <input
        id="llm-model"
        bind:value={settings.llm_model}
        onblur={onSave}
        placeholder="gpt-4o-mini"
      />
    </div>
  </div>
  <p class="caption">
    Used by “Send to LLM”. OpenAI needs <code>OPENAI_API_KEY</code> in the
    environment.
  </p>
</section>
