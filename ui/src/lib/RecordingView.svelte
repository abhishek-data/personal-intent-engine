<script>
  // The result, set as a dictionary entry: what was heard, how it was
  // corrected, the sense PIE took from it, and the rendered prompt.
  //
  // Your speech sets in the serif because it is human language; the prompt
  // and the model's reply set in mono because they are machine output.
  import { invoke } from "@tauri-apps/api/core";
  import { keycaps } from "./keycaps.js";
  import Icon from "./Icon.svelte";

  let {
    recState, outcome, llmResponse, llmBusy, hotkey, stateLabel,
    level = 0, hasLevel = false,
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
      <article class="entry">
        <section class="leaf">
          <div class="leaf-head">
            <span class="leaf-label">Heard</span>
            <span class="leaf-rule"></span>
          </div>
          <p class="said">{outcome.transcript}</p>

          {#if outcome.applied && outcome.applied.length}
            <div class="marks">
              {#each outcome.applied as f}
                <span class="mark">
                  <span class="mark-heard">{f.from}</span>
                  <span class="mark-arrow"><Icon name="arrow" size={13} /></span>
                  <span class="mark-canon">{f.to}</span>
                  {#if f.tier === "Llm"}
                    <button
                      class="mark-save"
                      onclick={() => saveFix(f)}
                      disabled={saved[f.from]}
                      aria-label={`Save correction: ${f.from} to ${f.to}`}
                    >{saved[f.from] ? "Saved" : "Save"}</button>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}

          <div class="actions">
            <button
              class="text-btn"
              onclick={onRecorrect}
              disabled={llmBusy}
              aria-label="Re-correct with AI"
            >Re-correct with AI</button>
          </div>
        </section>

        <section class="leaf">
          <div class="leaf-head">
            <span class="leaf-label">Understood</span>
            <span class="leaf-rule"></span>
          </div>
          {#if outcome.objective}
            <p class="sense">{outcome.objective}</p>
          {/if}
          <p class="usage">
            <span>{outcome.conversation_type}</span>
            <span>{outcome.confidence} confidence</span>
          </p>
        </section>

        <section class="leaf">
          <div class="leaf-head">
            <span class="leaf-label">Optimized prompt</span>
            <span class="leaf-rule"></span>
            <span class="leaf-meta">{outcome.mode} · ~{outcome.estimated_tokens} tokens</span>
          </div>
          <pre class="machine prompt">{outcome.optimized_prompt}</pre>
          <div class="actions">
            <button class="btn" onclick={onSend} disabled={llmBusy} aria-label="Send to LLM">
              {llmBusy ? "Sending…" : "Send to LLM"}
            </button>
            <button class="btn ghost" onclick={onCopy} aria-label="Copy prompt">Copy</button>
          </div>
        </section>

        {#if llmResponse}
          <section class="leaf">
            <div class="leaf-head">
              <span class="leaf-label">Response</span>
              <span class="leaf-rule"></span>
            </div>
            <pre class="machine response">{llmResponse}</pre>
          </section>
        {/if}
      </article>
    {:else}
      <!-- A reference book explains how to read an entry before you meet one.
           This teaches the result's structure and gives the empty state
           something to be besides a void. -->
      <div class="placeholder">
        <p class="placeholder-lead">Press record or your hotkey to start.</p>
        <dl class="guide">
          <div>
            <dt>Heard</dt>
            <dd>what you said, with every jargon fix shown</dd>
          </div>
          <div>
            <dt>Understood</dt>
            <dd>the objective PIE took from it</dd>
          </div>
          <div>
            <dt>Optimized prompt</dt>
            <dd>the text that lands at your cursor</dd>
          </div>
        </dl>
        <p class="placeholder-note">
          Speech is transcribed on this machine. Nothing is sent anywhere
          unless you send it.
        </p>
      </div>
    {/if}
  </div>

  <div
    class="record-bar"
    class:is-recording={recState === "recording"}
    class:is-decoding={recState === "decoding"}
    class:has-level={hasLevel && recState === "recording"}
    style="--level:{level}"
  >
    <button
      class="record-btn {recState}"
      onclick={onToggle}
      disabled={recState === "decoding"}
      aria-label={stateLabel}
    >
      <span class="record-mark"></span>
    </button>

    <div class="record-status">
      <span class="record-state">{stateLabel}</span>
      <div class="live-rule"></div>
      <p class="record-hint">
        {#if recState === "recording"}
          <button class="text-btn" onclick={onCancel} aria-label="Cancel recording">Cancel</button>
        {:else if caps.length}
          <span>or press</span>
          <span class="keys">{#each caps as cap}<kbd>{cap}</kbd>{/each}</span>
          <span>in any app</span>
        {/if}
      </p>
    </div>
  </div>
</div>
