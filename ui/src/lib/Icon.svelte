<script>
  // PIE's icon set. Drawn here rather than pulled from a library so every
  // glyph shares one stroke weight and the world's square terminals — the
  // interface has no rounded corners, so its icons have none either.
  let { name, size = 14, stroke = 1.4 } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="square"
  stroke-linejoin="miter"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "arrow"}
    <path d="M2.5 8h10M9 4.5 12.5 8 9 11.5" />
  {:else if name === "chevron-right"}
    <path d="M6 3.5 10.5 8 6 12.5" />
  {:else if name === "chevron-down"}
    <path d="M3.5 6 8 10.5 12.5 6" />
  {:else if name === "close"}
    <path d="M3.5 3.5l9 9M12.5 3.5l-9 9" />
  {:else if name === "trash"}
    <path d="M2.5 4.5h11M6 4.5V2.5h4v2M6.5 7.5v4M9.5 7.5v4M3.75 4.5l.6 8.2h7.3l.6-8.2" />
  {:else if name === "search"}
    <circle cx="7" cy="7" r="4.25" />
    <path d="M10.25 10.25 13.5 13.5" />
  {:else if name === "check"}
    <path d="M3 8.5 6.25 11.75 13 5" />
  {:else if name === "plus"}
    <path d="M8 3v10M3 8h10" />
  {/if}
</svg>
