<script>
  import { invoke } from "@tauri-apps/api/core";

  // Shortcut pane: click "Change", press a combo; we read event.code +
  // modifiers (which the Tauri shortcut parser accepts verbatim) and save it.
  // The current binding is suspended while capturing so it doesn't fire on the
  // keys being chosen.
  let { settings, onSave, onError } = $props();

  let capturingHotkey = $state(false);
  const MODIFIER_CODES = [
    "MetaLeft", "MetaRight", "ControlLeft", "ControlRight",
    "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "CapsLock",
  ];

  async function beginCaptureHotkey() {
    onError("");
    try {
      await invoke("set_hotkey_active", { active: false });
      capturingHotkey = true;
    } catch (e) {
      onError(String(e));
    }
  }

  async function endCapture(newHotkey) {
    capturingHotkey = false;
    if (newHotkey === null) {
      // Cancelled: restore the existing binding.
      await invoke("set_hotkey_active", { active: true }).catch(() => {});
      return;
    }
    settings.hotkey = newHotkey;
    await onSave(); // update_settings re-registers the new hotkey
  }

  function onHotkeyCapture(e) {
    if (!capturingHotkey) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") return endCapture(null);
    if (MODIFIER_CODES.includes(e.code)) return; // wait for a real key
    if (!e.code || e.code === "Unidentified") return;

    const mods = [];
    if (e.metaKey) mods.push("Command");
    if (e.ctrlKey) mods.push("Control");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");

    const isFunctionKey = /^F\d{1,2}$/.test(e.code);
    // A global shortcut with no modifier fires on every plain keypress
    // system-wide — only allow it for function keys.
    if (mods.length === 0 && !isFunctionKey) return;

    endCapture([...mods, e.code].join("+"));
  }

  function resetHotkey() {
    settings.hotkey = "CmdOrCtrl+Shift+Space";
    onSave();
  }

  function disableHotkey() {
    settings.hotkey = "";
    onSave();
  }

  // Turn a stored accelerator into display keycaps (⌘ ⇧ Space, etc.).
  const CAP_SYMBOLS = {
    Command: "⌘", Cmd: "⌘", CmdOrCtrl: "⌘", CommandOrControl: "⌘",
    Super: "⌘", Meta: "⌘", Control: "⌃", Ctrl: "⌃", Alt: "⌥",
    Option: "⌥", Shift: "⇧",
  };
  const ARROW_SYMBOLS = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→" };
  function keycaps(accel) {
    return accel.split("+").map((t) => {
      if (CAP_SYMBOLS[t]) return CAP_SYMBOLS[t];
      if (t.startsWith("Key")) return t.slice(3);
      if (t.startsWith("Digit")) return t.slice(5);
      if (ARROW_SYMBOLS[t]) return ARROW_SYMBOLS[t];
      return t;
    });
  }
</script>

<svelte:window onkeydown={onHotkeyCapture} />

<section class="group">
  <div class="field">
    <span class="field-label">Global hotkey</span>
    <div class="hotkey-row">
      <div class="hotkey-display" class:capturing={capturingHotkey}>
        {#if capturingHotkey}
          <span class="capture-hint">Press a combo…</span>
        {:else if settings.hotkey}
          {#each keycaps(settings.hotkey) as cap}
            <kbd>{cap}</kbd>
          {/each}
        {:else}
          <span class="muted">Disabled</span>
        {/if}
      </div>
      {#if capturingHotkey}
        <button class="btn ghost" onclick={() => endCapture(null)} aria-label="Cancel capturing hotkey">
          Cancel
        </button>
      {:else}
        <button class="btn" onclick={beginCaptureHotkey} aria-label="Change global hotkey">
          Change
        </button>
      {/if}
    </div>
    {#if capturingHotkey}
      <p class="caption">
        Press the keys you want, e.g. <kbd>⌘</kbd><kbd>⇧</kbd>Space.
        <kbd>Esc</kbd> cancels.
      </p>
    {:else}
      <div class="hotkey-actions">
        <button class="text-btn" onclick={resetHotkey}>Reset to default</button>
        <button class="text-btn" onclick={disableHotkey}>Disable</button>
      </div>
      <p class="caption">
        Press it in any app to start recording; press again to stop and paste.
        First use needs Accessibility permission on macOS.
      </p>
    {/if}
  </div>
</section>
