<script>
  // Home pane: the record button plus the result card
  // (speech → intent → prompt). All state lives in the parent; this component
  // only renders it and reports button clicks through callbacks.
  let {
    recState,
    outcome,
    llmResponse,
    llmBusy,
    hotkey,
    stateLabel,
    onToggle,
    onCancel,
    onSend,
    onCopy,
  } = $props();
</script>

<section class="record-hero" class:centered={!outcome}>
  <button
    class="record {recState}"
    onclick={onToggle}
    disabled={recState === "decoding"}
    aria-label={stateLabel}
  >
    <span class="dot"></span>
  </button>
  <p class="record-state">{stateLabel}</p>
  <p class="record-hint">
    or press <kbd>{hotkey}</kbd> in any app
  </p>
  {#if recState === "recording"}
    <button class="text-btn" onclick={onCancel} aria-label="Cancel recording">
      Cancel
    </button>
  {/if}
</section>

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
        <button
          class="btn"
          onclick={onSend}
          disabled={llmBusy}
          aria-label="Send optimized prompt to the LLM"
        >
          {llmBusy ? "Sending…" : "Send to LLM"}
        </button>
        <button class="btn ghost" onclick={onCopy} aria-label="Copy optimized prompt">
          Copy
        </button>
      </div>
    </div>

    {#if llmResponse}
      <div class="result-step">
        <span class="eyebrow">Response</span>
        <pre class="response">{llmResponse}</pre>
      </div>
    {/if}
  </section>
{/if}
