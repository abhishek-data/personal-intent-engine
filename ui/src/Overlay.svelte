<script>
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  // Driven by the same pie://state events as the main window. The Rust side
  // only shows the overlay window for the recording/decoding states, but we
  // track state here to pick the right dot color, label, and animation.
  let state = $state("recording");

  onMount(async () => {
    await listen("pie://state", (event) => {
      state = event.payload;
    });
  });

  const label = $derived(
    { recording: "Recording", decoding: "Transcribing…" }[state] ?? state,
  );
</script>

<div class="pill {state}">
  <span class="dot"></span>
  <span class="label">{label}</span>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    /* Overlay is display-only — never intercept clicks meant for the app
       underneath. */
    user-select: none;
    -webkit-user-select: none;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    margin: 8px;
    padding: 0.5rem 0.9rem;
    border-radius: 999px;
    background: rgba(20, 20, 26, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(12px);
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #e8e8ec;
    width: fit-content;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #e5484d;
    flex-shrink: 0;
  }
  .pill.recording .dot {
    animation: blink 1.2s ease-in-out infinite;
  }
  .pill.decoding .dot {
    background: #f0b429;
    animation: pulse 0.9s ease-in-out infinite;
  }
  .label {
    font-size: 0.85rem;
    font-weight: 500;
    white-space: nowrap;
  }
  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
  @keyframes pulse {
    50% {
      transform: scale(0.7);
    }
  }
</style>
