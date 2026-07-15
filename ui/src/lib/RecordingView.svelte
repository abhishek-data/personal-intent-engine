<script>
  let { recState, outcome, llmResponse, llmBusy, hotkey, stateLabel, onToggle, onCancel, onSend, onCopy } = $props();
</script>

<div class="record-view">
  <div class="record-scroll">
    {#if outcome}
      <section class="result">
        <div class="result-step">
          <span class="eyebrow">Heard</span>
          <p class="transcript">{outcome.transcript}</p>
        </div>

        <div class="result-step">
          <span class="eyebrow">Understood</span>
          <div class="chips">
            <span class="chip">{outcome.conversation_type}</span>
            <span class="chip">{outcome.confidence} confidence</span>
            {#if outcome.objective}
              <span class="chip objective">{outcome.objective}</span>
            {/if}
          </div>
        </div>

        <div class="result-step">
          <div class="step-head">
            <span class="eyebrow">Optimized prompt</span>
            <span class="muted">{outcome.mode} · ~{outcome.estimated_tokens} tokens</span>
          </div>
          <pre class="prompt">{outcome.optimized_prompt}</pre>
          <div class="actions">
            <button class="btn" onclick={onSend} disabled={llmBusy} aria-label="Send to LLM">
              {llmBusy ? "Sending…" : "Send to LLM"}
            </button>
            <button class="btn ghost" onclick={onCopy} aria-label="Copy prompt">Copy</button>
          </div>
        </div>

        {#if llmResponse}
          <div class="result-step">
            <span class="eyebrow">Response</span>
            <pre class="response">{llmResponse}</pre>
          </div>
        {/if}
      </section>
    {:else}
      <div class="record-placeholder">Press record or your hotkey to start.</div>
    {/if}
  </div>

  <div class="record-bar">
    <button
      class="record-btn {recState}"
      onclick={onToggle}
      disabled={recState === "decoding"}
      aria-label={stateLabel}
    >
      <span class="dot"></span>
    </button>
    <p class="record-state">{stateLabel}</p>
    <p class="record-hint">or press <kbd>{hotkey}</kbd> in any app</p>
    {#if recState === "recording"}
      <button class="text-btn" onclick={onCancel} aria-label="Cancel recording">Cancel</button>
    {/if}
  </div>
</div>
