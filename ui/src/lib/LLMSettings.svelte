<script>
  // LLM Provider: endpoint, key, and model used by "Send to LLM".
  import { invoke } from "@tauri-apps/api/core";

  let { settings, onSave } = $props();

  let testing = $state(false);
  let testResult = $state("");
  let testOk = $state(false);

  // If the user types a real endpoint while still on the "echo" debug
  // provider, promote them to "openai" so "Send to LLM" actually calls out
  // instead of silently echoing. Only fires when the URL is non-empty, so
  // clearing the URL never changes the provider.
  function onUrlChange() {
    if (settings.llm_api_url.trim() !== "" && settings.provider === "echo") {
      settings.provider = "openai";
    }
    onSave();
  }

  async function testConnection() {
    testing = true;
    testResult = "";
    try {
      await invoke("test_llm_connection", {
        url: settings.llm_api_url,
        key: settings.llm_api_key,
        model: settings.llm_model,
      });
      testOk = true;
      testResult = "Connected";
    } catch (e) {
      testOk = false;
      testResult = String(e);
    } finally {
      testing = false;
    }
  }
</script>

<section class="leaf">
  <div class="leaf-head">
    <span class="leaf-label">LLM provider</span>
    <span class="leaf-rule"></span>
  </div>

  <div class="field">
    <label for="provider">LLM provider</label>
    <select id="provider" bind:value={settings.provider} onchange={onSave}>
      <option value="echo">Echo (debug)</option>
      <option value="openai">OpenAI</option>
      <option value="openrouter">OpenRouter</option>
    </select>
  </div>

  <div class="field">
    <label for="llm-url">API URL</label>
    <input
      id="llm-url"
      bind:value={settings.llm_api_url}
      onblur={onUrlChange}
      placeholder="https://api.openai.com/v1"
    />
  </div>

  <div class="field">
    <label for="llm-key">API key</label>
    <input
      id="llm-key"
      type="password"
      bind:value={settings.llm_api_key}
      onblur={onSave}
      placeholder="sk-…"
    />
  </div>

  <div class="field">
    <label for="llm-provider-model">Model</label>
    <input
      id="llm-provider-model"
      bind:value={settings.llm_model}
      onblur={onSave}
      placeholder="gpt-4o-mini"
    />
  </div>

  <div class="field">
    <button class="btn ghost sm" disabled={testing} onclick={testConnection}>
      {testing ? "Testing…" : "Test Connection"}
    </button>
    {#if testResult}
      <p class="note" class:is-proof={!testOk}>{testResult}</p>
    {/if}
    <p class="note">
      Endpoint, key, and model for “Send to LLM”. Leave blank to fall back to
      environment variables (e.g. <code>OPENAI_API_KEY</code>).
    </p>
  </div>
</section>
