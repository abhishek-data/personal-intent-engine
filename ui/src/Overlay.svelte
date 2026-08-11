<script>
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let state = $state("recording");

  onMount(() => {
    let unlisten;
    listen("pie://state", (event) => {
      state = event.payload;
    }).then((u) => { unlisten = u; });
    return () => { unlisten?.(); };
  });

  const label = $derived(
    { recording: "Recording", decoding: "Transcribing…" }[state] ?? state
  );
</script>

<div class="pill {state}">
  <span class="dot"></span>
  <span class="label">{label}</span>
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgba(15, 15, 19, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(12px);
    font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif;
    color: #e8e8ec;
    width: fit-content;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #ef4444;
    flex-shrink: 0;
  }
  .pill.recording .dot { animation: blink 1.2s ease-in-out infinite; }
  .pill.decoding .dot {
    background: #f59e0b;
    animation: pulse 0.9s ease-in-out infinite;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }
  @keyframes blink { 50% { opacity: 0.35; } }
  @keyframes pulse { 50% { transform: scale(0.7); } }
</style>
