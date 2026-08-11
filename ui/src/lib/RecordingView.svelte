<script>
  import { invoke } from "@tauri-apps/api/core";
  import { keycaps } from "./keycaps.js";

  let {
    recState, outcome, llmResponse, llmBusy, hotkey, stateLabel,
    onToggle, onCancel, onSend, onCopy, onRecorrect, onError,
  } = $props();

  // Render the user's actual configured hotkey as keycaps (⌘ ⇧ Space); when the
  // hotkey is disabled (empty) there is nothing to press, so the hint is hidden.
  const caps = $derived(keycaps(hotkey));

  // Saving a fix the AI made moves it into the user's own dictionary, so the
  // dictionary tier catches it instantly next time (no LLM round-trip needed).
  // Reset per-fix "Saved" flags whenever a new outcome replaces this one.
  let saved = $state({});
  $effect(() => { outcome; saved = {}; });

  async function saveFix(f) {
    try {
      await invoke("add_correction", { heard: f.from, canonical: f.to });
      saved = { ...saved, [f.from]: true };
    } catch (e) {
      onError?.(String(e));
    }
  }
</script>

<div class="record-view">
  <div class="record-scroll">
    {#if outcome}
      <section class="result">
        <div class="result-step">
          <span class="eyebrow">Heard</span>
          <p class="transcript">{outcome.transcript}</p>
          {#if outcome.applied && outcome.applied.length}
            <p class="corrected-note">
              corrected
              {#each outcome.applied as f}
                <span class="fix">
                  {f.from} → {f.to}
                  {#if f.tier === "Llm"}
                    <button
                      class="fix-save"
                      onclick={() => saveFix(f)}
                      disabled={saved[f.from]}
                      aria-label={`Save correction: ${f.from} to ${f.to}`}
                    >{saved[f.from] ? "Saved" : "Save"}</button>
                  {/if}
                </span>
              {/each}
            </p>
          {/if}
          <button
            class="text-btn"
            onclick={onRecorrect}
            disabled={llmBusy}
            aria-label="Re-correct with AI"
          >Re-correct with AI</button>
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
    {#if caps.length}
      <p class="record-hint">or press {#each caps as cap}<kbd>{cap}</kbd>{/each} in any app</p>
    {/if}
    {#if recState === "recording"}
      <button class="text-btn" onclick={onCancel} aria-label="Cancel recording">Cancel</button>
    {/if}
  </div>
</div>
