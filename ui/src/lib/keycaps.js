// Turn a stored accelerator (e.g. "CmdOrCtrl+Shift+Space") into display
// keycaps (["⌘", "⇧", "Space"]). Shared by the settings pane and the record
// view so the hotkey reads the same everywhere.
const CAP_SYMBOLS = {
  Command: "⌘", Cmd: "⌘", CmdOrCtrl: "⌘", CommandOrControl: "⌘",
  Super: "⌘", Meta: "⌘", Control: "⌃", Ctrl: "⌃", Alt: "⌥",
  Option: "⌥", Shift: "⇧",
};
const ARROW_SYMBOLS = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→" };

export function keycaps(accel) {
  if (!accel) return [];
  return accel.split("+").map((t) => {
    if (CAP_SYMBOLS[t]) return CAP_SYMBOLS[t];
    if (t.startsWith("Key")) return t.slice(3);
    if (t.startsWith("Digit")) return t.slice(5);
    if (ARROW_SYMBOLS[t]) return ARROW_SYMBOLS[t];
    return t;
  });
}
