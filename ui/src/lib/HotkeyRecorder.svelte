<script>
  import { invoke } from "@tauri-apps/api/core";
  import { keycaps } from "./keycaps.js";

  // Shortcut pane: click "Change", press a combo; we read event.code +
  // modifiers (which the Tauri shortcut parser accepts verbatim) and save it.
  // The current binding is suspended while capturing so it doesn't fire on the
  // keys being chosen.
  // `field` is the settings key this recorder edits (e.g. "hotkey_raw");
  // `label` is the shown title; `defaultValue` is used by "Reset to default".
  let {
    settings,
    onSave,
    onError,
    field = "hotkey_optimized",
    label = "Global hotkey",
    defaultValue = "CmdOrCtrl+Shift+Space",
    // Both recorders share the same explanation; showing it twice in a row
    // is noise, so the second instance suppresses it.
    showNote = true,
  } = $props();

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
    settings[field] = newHotkey;
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
    settings[field] = defaultValue;
    onSave();
  }

  function disableHotkey() {
    settings[field] = "";
    onSave();
  }
</script>

<svelte:window onkeydown={onHotkeyCapture} />

<section class="leaf">
  <div class="leaf-head">
    <span class="leaf-label">{label}</span>
    <span class="leaf-rule"></span>
  </div>

  <div class="field">
    <div class="hotkey-row">
      <div class="hotkey-display" class:capturing={capturingHotkey}>
        {#if capturingHotkey}
          <span class="capture-hint">Press a combo…</span>
        {:else if settings[field]}
          <span class="keys">
            {#each keycaps(settings[field]) as cap}<kbd>{cap}</kbd>{/each}
          </span>
        {:else}
          <span class="hotkey-off">Disabled</span>
        {/if}
      </div>
      {#if capturingHotkey}
        <button class="btn ghost sm" onclick={() => endCapture(null)} aria-label="Cancel capturing hotkey">
          Cancel
        </button>
      {:else}
        <button class="btn sm" onclick={beginCaptureHotkey} aria-label="Change global hotkey">
          Change
        </button>
      {/if}
    </div>

    {#if capturingHotkey}
      <p class="note">
        Press the keys you want, e.g. <kbd>⌘</kbd><kbd>⇧</kbd>Space.
        <kbd>Esc</kbd> cancels.
      </p>
    {:else}
      <div class="hotkey-actions">
        <button class="text-btn" onclick={resetHotkey}>Reset to default</button>
        <button class="text-btn" onclick={disableHotkey}>Disable</button>
      </div>
      {#if showNote}
        <p class="note">
          Press it in any app to start recording; press again to stop and paste.
          First use needs Accessibility permission on macOS.
        </p>
      {/if}
    {/if}
  </div>
</section>
