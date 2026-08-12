import { mount } from "svelte";
// The overlay shares the main window's design tokens — same ink, same rules,
// same typefaces. Without this the two surfaces drift apart.
import "./tokens.css";
import Overlay from "./Overlay.svelte";

const overlay = mount(Overlay, {
  target: document.getElementById("overlay"),
});

export default overlay;
