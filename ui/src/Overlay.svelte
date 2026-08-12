<script>
  // The floating recording slip.
  //
  // This is the surface people actually see: it appears over whatever app is
  // focused, while the main window is usually hidden in the tray. It reads
  // from the same token file as the main window, so the two are recognisably
  // one product.
  //
  // Liveness is carried by a rule that presses harder as you speak — an ink
  // stroke, not a blinking light. When the recorder is feeding real levels
  // (`pie://level`) the rule tracks your voice; without them it falls back to
  // a slow sweep so the slip never looks frozen.
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let state = $state("recording");
  let level = $state(0);
  let hasLevel = $state(false);

  onMount(() => {
    let unlisteners = [];
    let disposed = false;

    (async () => {
      const subs = await Promise.all([
        listen("pie://state", (event) => {
          state = event.payload;
          if (state !== "recording") level = 0;
        }),
        listen("pie://level", (event) => {
          hasLevel = true;
          level = Math.max(0, Math.min(1, Number(event.payload) || 0));
        }),
      ]);
      if (disposed) { subs.forEach((u) => u()); return; }
      unlisteners = subs;
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  });

  const label = $derived(
    { recording: "Recording", decoding: "Transcribing…" }[state] ?? state
  );
</script>

<div class="slip {state}" class:has-level={hasLevel && state === "recording"}>
  <div class="line">
    <span class="mark" aria-hidden="true"></span>
    <span class="label">{label}</span>
  </div>
  <div class="meter" style="--level:{level}"></div>
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
  }

  .slip {
    margin: 8px;
    padding: 11px 14px 9px;
    background: rgba(15, 14, 12, 0.97);
    border: 1px solid rgba(237, 231, 220, 0.22);
    border-radius: 0;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55), 0 1px 3px rgba(0, 0, 0, 0.6);
    font-family: var(--sans);
    color: var(--bone);
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .line { display: flex; align-items: center; gap: 10px; }

  /* Same mark vocabulary as the record button in the main window:
     a filled square while capturing, a muted dot while transcribing. */
  .mark {
    width: 9px;
    height: 9px;
    flex-shrink: 0;
    background: var(--proof);
  }
  .decoding .mark { background: var(--bone-4); border-radius: 50%; }

  .label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  /* The rule. Real level when the recorder feeds one; a sweep otherwise. */
  .meter { height: 2px; background: var(--rule); position: relative; overflow: hidden; }
  .meter::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--proof);
    transform-origin: left center;
    transform: scaleX(0);
  }
  .recording .meter::after { animation: sweep 1.6s cubic-bezier(0.2, 0, 0, 1) infinite; }
  .recording.has-level .meter::after {
    animation: none;
    transform: scaleX(var(--level));
    transition: transform 90ms linear;
  }
  .decoding .meter {
    background: repeating-linear-gradient(90deg, var(--rule-strong) 0 4px, transparent 4px 8px);
  }

  @keyframes sweep {
    0%   { transform: scaleX(0); opacity: 1; }
    70%  { transform: scaleX(1); opacity: 1; }
    100% { transform: scaleX(1); opacity: 0; }
  }

  @media (prefers-reduced-motion: reduce) {
    .recording .meter::after {
      animation: none !important;
      transform: scaleX(1);
      transition: none;
    }
  }
</style>
